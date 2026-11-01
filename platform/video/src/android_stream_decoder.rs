//! Bounded Annex-B H.264 decoder backed by MediaCodec and an ImageReader.
//! The owner calls this on its decode worker, never from the UI thread.
use crate::{annex_b, stream_decoder::DecodedFrame, StreamVideoCodec, VideoFileError};
use std::{ffi::{c_char,c_void}, ptr};

type Error = VideoFileError;
#[repr(C)] struct BufferInfo { offset:i32, size:i32, presentation_time_us:i64, flags:u32 }
#[repr(C)] #[derive(Default)] struct Crop { left:i32, top:i32, right:i32, bottom:i32 }
#[link(name="mediandk")]
extern "C" {
 fn AMediaCodec_createDecoderByType(mime:*const c_char)->*mut c_void;
 fn AMediaCodec_delete(codec:*mut c_void)->i32;
 fn AMediaCodec_configure(codec:*mut c_void,format:*const c_void,surface:*mut c_void,crypto:*mut c_void,flags:u32)->i32;
 fn AMediaCodec_start(codec:*mut c_void)->i32;
 fn AMediaCodec_stop(codec:*mut c_void)->i32;
 fn AMediaCodec_dequeueInputBuffer(codec:*mut c_void,timeout:i64)->isize;
 fn AMediaCodec_getInputBuffer(codec:*mut c_void,index:usize,size:*mut usize)->*mut u8;
 fn AMediaCodec_queueInputBuffer(codec:*mut c_void,index:usize,offset:usize,size:usize,time:u64,flags:u32)->i32;
 fn AMediaCodec_dequeueOutputBuffer(codec:*mut c_void,info:*mut BufferInfo,timeout:i64)->isize;
 fn AMediaCodec_releaseOutputBuffer(codec:*mut c_void,index:usize,render:bool)->i32;
 fn AMediaFormat_new()->*mut c_void;
 fn AMediaFormat_delete(format:*mut c_void)->i32;
 fn AMediaFormat_setString(format:*mut c_void,name:*const c_char,value:*const c_char);
 fn AMediaFormat_setInt32(format:*mut c_void,name:*const c_char,value:i32);
 fn AImageReader_new(w:i32,h:i32,format:u32,max:i32,reader:*mut *mut c_void)->i32;
 fn AImageReader_getWindow(reader:*mut c_void,window:*mut *mut c_void)->i32;
 fn AImageReader_delete(reader:*mut c_void);
 fn AImageReader_acquireLatestImage(reader:*mut c_void,image:*mut *mut c_void)->i32;
 fn AImage_delete(image:*mut c_void);
 fn AImage_getWidth(image:*const c_void,out:*mut i32)->i32;
 fn AImage_getHeight(image:*const c_void,out:*mut i32)->i32;
 fn AImage_getCropRect(image:*const c_void,out:*mut Crop)->i32;
 fn AImage_getTimestamp(image:*const c_void,out:*mut i64)->i32;
 fn AImage_getPlaneData(image:*const c_void,plane:i32,data:*mut *mut u8,len:*mut i32)->i32;
 fn AImage_getPlaneRowStride(image:*const c_void,plane:i32,out:*mut i32)->i32;
 fn AImage_getPlanePixelStride(image:*const c_void,plane:i32,out:*mut i32)->i32;
}
fn check(status:i32,what:&str)->Result<(),Error>{if status==0{Ok(())}else{Err(Error::new(format!("{what}: MediaCodec status {status}")))}}
struct Image(*mut c_void);
impl Drop for Image {fn drop(&mut self){unsafe{AImage_delete(self.0)}}}

pub(crate) struct AndroidStreamDecoder {codec:*mut c_void, reader:*mut c_void, dimensions:Option<(u32,u32)>}
// The codec/reader are transferred as a unit; all operations remain on one worker.
unsafe impl Send for AndroidStreamDecoder {}
impl Drop for AndroidStreamDecoder {fn drop(&mut self){self.close();}}
impl AndroidStreamDecoder {
 pub fn new(codec:StreamVideoCodec)->Result<Self,Error>{
  if codec!=StreamVideoCodec::H264{return Err(Error::new("Android live decoding currently supports H.264"))}
  Ok(Self{codec:ptr::null_mut(),reader:ptr::null_mut(),dimensions:None})
 }
 fn close(&mut self){unsafe{
  if !self.codec.is_null(){AMediaCodec_stop(self.codec);AMediaCodec_delete(self.codec);self.codec=ptr::null_mut();}
  if !self.reader.is_null(){AImageReader_delete(self.reader);self.reader=ptr::null_mut();}
 };self.dimensions=None;}
 fn configure(&mut self,w:u32,h:u32)->Result<(),Error>{
  self.close();
  let result=(||unsafe{
   check(AImageReader_new(w as i32,h as i32,35,3,&mut self.reader),"create image reader")?;
   let mut surface=ptr::null_mut();check(AImageReader_getWindow(self.reader,&mut surface),"image reader surface")?;
   self.codec=AMediaCodec_createDecoderByType(b"video/avc\0".as_ptr().cast());
   if self.codec.is_null(){return Err(Error::new("No Android H.264 decoder"))}
   let format=AMediaFormat_new();if format.is_null(){return Err(Error::new("Cannot create decoder format"))}
   AMediaFormat_setString(format,b"mime\0".as_ptr().cast(),b"video/avc\0".as_ptr().cast());
   AMediaFormat_setInt32(format,b"width\0".as_ptr().cast(),w as i32);
   AMediaFormat_setInt32(format,b"height\0".as_ptr().cast(),h as i32);
   AMediaFormat_setInt32(format,b"max-input-size\0".as_ptr().cast(),16*1024*1024);
   AMediaFormat_setInt32(format,b"low-latency\0".as_ptr().cast(),1);
   let status=AMediaCodec_configure(self.codec,format,surface,ptr::null_mut(),0);
   AMediaFormat_delete(format);check(status,"configure live decoder")?;
   check(AMediaCodec_start(self.codec),"start live decoder")?;
   self.dimensions=Some((w,h));Ok(())
  })();if result.is_err(){self.close();}result
 }
 pub fn push_packet(&mut self,data:&[u8],pts_100ns:i64)->Result<Vec<DecodedFrame>,Error>{
  if data.is_empty()||data.len()>16*1024*1024{return Err(Error::new("Live access unit outside size limit"))}
  for nal in annex_b::split_annex_b(data){if annex_b::nal_unit_type(nal)==7{
   let size=sps_size(nal)?;if self.dimensions!=Some(size){self.configure(size.0,size.1)?;}
  }}
  if self.codec.is_null(){return Ok(Vec::new())}
  let mut frames=self.flush()?;
  unsafe{
   let index=AMediaCodec_dequeueInputBuffer(self.codec,10_000);
   if index<0{return Err(Error::new("Live decoder input backpressure; restart at a keyframe"))}
   let mut size=0;let buffer=AMediaCodec_getInputBuffer(self.codec,index as usize,&mut size);
   if buffer.is_null()||size<data.len(){return Err(Error::new("Live decoder input buffer is too small"))}
   ptr::copy_nonoverlapping(data.as_ptr(),buffer,data.len());
   check(AMediaCodec_queueInputBuffer(self.codec,index as usize,0,data.len(),pts_100ns.max(0) as u64/10,0),"queue live access unit")?;
  }
  frames.extend(self.flush()?);if frames.len()>1{frames.drain(..frames.len()-1);}Ok(frames)
 }
 /// Poll available decoded output; does not signal EOS or wait for a frame.
 pub fn flush(&mut self)->Result<Vec<DecodedFrame>,Error>{
  if self.codec.is_null(){return Ok(Vec::new())}
  unsafe{for _ in 0..8{
   let mut info=BufferInfo{offset:0,size:0,presentation_time_us:0,flags:0};
   let index=AMediaCodec_dequeueOutputBuffer(self.codec,&mut info,0);
   if index>=0{check(AMediaCodec_releaseOutputBuffer(self.codec,index as usize,true),"release decoded frame")?;break;}
   else if index == -1{break;}else if index != -2 && index != -3{return Err(Error::new(format!("Live decoder output status {index}")))}
  }}
  let mut raw=ptr::null_mut();let status=unsafe{AImageReader_acquireLatestImage(self.reader,&mut raw)};
  if status == -30001{return Ok(Vec::new())} // AMEDIA_IMGREADER_NO_BUFFER_AVAILABLE
  check(status,"acquire decoded image")?;if raw.is_null(){return Err(Error::new("Decoder returned null image"))}
  let image=Image(raw);let mut w=0;let mut h=0;let mut crop=Crop::default();let mut time=0;
  unsafe{check(AImage_getWidth(image.0,&mut w),"image width")?;check(AImage_getHeight(image.0,&mut h),"image height")?;
   check(AImage_getCropRect(image.0,&mut crop),"image crop")?;check(AImage_getTimestamp(image.0,&mut time),"image timestamp")?;}
  if w<=0||h<=0||w>4096||h>4096||crop.left<0||crop.top<0||crop.right>w||crop.bottom>h||crop.right<=crop.left||crop.bottom<=crop.top||crop.left%2!=0||crop.top%2!=0{
   return Err(Error::new("Unsupported decoded image bounds"))
  }
  let width=(crop.right-crop.left) as usize;let height=(crop.bottom-crop.top) as usize;
  if width%2!=0||height%2!=0{return Err(Error::new("Live NV12 output requires even dimensions"))}
  let mut out=vec![0u8;width*height*3/2];
  for plane in 0..3{
   let mut data=ptr::null_mut();let mut len=0;let mut stride=0;let mut step=0;
   unsafe{check(AImage_getPlaneData(image.0,plane,&mut data,&mut len),"image plane")?;
    check(AImage_getPlaneRowStride(image.0,plane,&mut stride),"image row stride")?;
    check(AImage_getPlanePixelStride(image.0,plane,&mut step),"image pixel stride")?;}
   if data.is_null()||len<=0||stride<=0||step<=0{return Err(Error::new("Invalid decoded image plane"))}
   let div=if plane==0{1}else{2};let pw=width/div;let ph=height/div;
   let x=crop.left as usize/div;let y=crop.top as usize/div;
   let end=(y+ph-1).checked_mul(stride as usize).and_then(|n|n.checked_add((x+pw-1)*step as usize)).ok_or_else(||Error::new("Image stride overflow"))?;
   if end>=len as usize{return Err(Error::new("Decoded plane exceeds buffer"))}
   let source=unsafe{std::slice::from_raw_parts(data,len as usize)};
   for row in 0..ph{for col in 0..pw{
    let byte=source[(y+row)*stride as usize+(x+col)*step as usize];
    let dst=if plane==0{row*width+col}else{width*height+row*width+col*2+(plane as usize-1)};out[dst]=byte;
   }}
  }
  Ok(vec![DecodedFrame{width:width as u32,height:height as u32,nv12:out,pts_100ns:time/100}])
 }
}

// Only geometry is consumed; no codec metadata is trusted for allocations until bounded.
struct Bits{data:Vec<u8>,bit:usize}
impl Bits{
 fn read(&mut self,n:usize)->Result<u32,Error>{if n>32||self.bit+n>self.data.len()*8{return Err(Error::new("Truncated H.264 SPS"))}let mut v=0;for _ in 0..n{v=(v<<1)|((self.data[self.bit/8]>>(7-self.bit%8))&1) as u32;self.bit+=1;}Ok(v)}
 fn ue(&mut self)->Result<u32,Error>{let mut zeros=0;while self.read(1)?==0{zeros+=1;if zeros>24{return Err(Error::new("Oversized SPS integer"))}}Ok((1u32<<zeros)-1+self.read(zeros)?)}
 fn se(&mut self)->Result<i32,Error>{let v=self.ue()?;Ok(if v&1==0{-(v as i32/2)}else{(v as i32+1)/2})}
}
fn sps_size(nal:&[u8])->Result<(u32,u32),Error>{
 if nal.len()<5||nal.len()>65536{return Err(Error::new("Invalid SPS size"))}
 let mut rbsp=Vec::with_capacity(nal.len());let mut zeros=0;
 for &byte in &nal[1..]{if zeros>=2&&byte==3{zeros=0;continue}rbsp.push(byte);zeros=if byte==0{zeros+1}else{0};}
 let mut b=Bits{data:rbsp,bit:0};let profile=b.read(8)?;b.read(16)?;b.ue()?;
 let mut chroma=1;
 if matches!(profile,100|110|122|244|44|83|86|118|128|138|139|134|135){
  chroma=b.ue()?;if chroma>3{return Err(Error::new("Invalid SPS chroma"))}if chroma==3{b.read(1)?;}
  let depth=b.ue()?;let chroma_depth=b.ue()?;if depth!=0||chroma_depth!=0{return Err(Error::new("Only 8-bit live H.264 is supported"))}
  b.read(1)?;if b.read(1)?!=0{for i in 0..if chroma==3{12}else{8}{if b.read(1)?!=0{let mut last=8;let mut next=8;for _ in 0..if i<6{16}else{64}{if next!=0{next=(last+b.se()?+256)%256;}if next!=0{last=next;}}}}}
 }
 if chroma!=1{return Err(Error::new("Only 4:2:0 live H.264 is supported"))}
 b.ue()?;let order=b.ue()?;if order==0{b.ue()?;}else if order==1{b.read(1)?;b.se()?;b.se()?;let count=b.ue()?;if count>256{return Err(Error::new("Invalid SPS cycle"))}for _ in 0..count{b.se()?;}}else if order>2{return Err(Error::new("Invalid SPS order"))}
 b.ue()?;b.read(1)?;let mbw=b.ue()?+1;let mbh=b.ue()?+1;let frame=b.read(1)?;if frame==0{b.read(1)?;}b.read(1)?;
 let(mut left,mut right,mut top,mut bottom)=(0,0,0,0);if b.read(1)?!=0{left=b.ue()?;right=b.ue()?;top=b.ue()?;bottom=b.ue()?;}
 let w=mbw.checked_mul(16).and_then(|v|v.checked_sub((left+right)*2)).ok_or_else(||Error::new("Invalid SPS width"))?;
 let h=mbh.checked_mul(16*(2-frame)).and_then(|v|v.checked_sub((top+bottom)*2*(2-frame))).ok_or_else(||Error::new("Invalid SPS height"))?;
 if w==0||h==0||w>4096||h>4096||w%2!=0||h%2!=0{return Err(Error::new("H.264 dimensions outside supported bounds"))}Ok((w,h))
}
