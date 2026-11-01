// Application-owned keyboard policy; no platform or Makepad dependencies.
pub fn quantized(offset: f64, step: f64) -> f64 {
    let units = offset / step;
    let nearest = units.round();
    let units = if (units-nearest).abs() <= 8.0*f64::EPSILON*units.abs().max(1.0) {nearest} else {units.floor()};
    units * step
}
pub fn slider(value:f64, min:f64, max:f64, step:f64, key:&str)->Option<f64> {
    if ![value,min,max,step].iter().all(|v|v.is_finite()) || max<=min || step<0.0 {return None;}
    let increment=if step>0.0 {step} else {(max-min)/100.0};
    let next=match key {
        "ArrowUp"|"ArrowRight"=>value+increment,
        "ArrowDown"|"ArrowLeft"=>value-increment,
        "PageUp"=>value+increment*10.0, "PageDown"=>value-increment*10.0,
        "Home"=>min, "End"=>max, _=>return None,
    }.clamp(min,max);
    Some(if step>0.0 { (min+quantized(next-min,step)).clamp(min,max) } else {next})
}
pub fn scroll(value:f64,total:f64,visible:f64,horizontal:bool,key:&str)->Option<f64> {
    if ![value,total,visible].iter().all(|v|v.is_finite()) {return None;}
    let end=(total-visible).max(0.0);
    let next=match key {
        "ArrowUp" if !horizontal=>value-48.0,
        "ArrowDown" if !horizontal=>value+48.0,
        "ArrowLeft" if horizontal=>value-48.0,
        "ArrowRight" if horizontal=>value+48.0,
        "PageUp"=>value-visible*0.9, "PageDown"=>value+visible*0.9,
        "Home"=>0.0, "End"=>end, _=>return None,
    };
    Some(next.clamp(0.0,end))
}
#[cfg(test)] mod tests {
 use super::*;
 #[test] fn slider_steps_bounds_and_fractional_roundtrip(){
  let mut v=2.0;for _ in 0..10 {v=slider(v,2.0,3.0,0.1,"ArrowUp").unwrap();
   let relative=(v-2.0)/(3.0-2.0);v=2.0+quantized(relative*(3.0-2.0),0.1);}
  assert!((v-3.0).abs()<1e-12);
  for _ in 0..10 {v=slider(v,2.0,3.0,0.1,"ArrowDown").unwrap();}assert!((v-2.0).abs()<1e-12);
  assert_eq!(slider(12.0,8.0,32.0,1.0,"ArrowUp"),Some(13.0));
  assert_eq!(slider(8.0,8.0,32.0,1.0,"ArrowDown"),Some(8.0));
  assert_eq!(slider(50.0,0.0,100.0,0.0,"ArrowDown"),Some(49.0));
  assert_eq!(slider(12.0,8.0,32.0,1.0,"End"),Some(32.0));
  assert_eq!(slider(1.0,1.0,1.0,0.0,"ArrowUp"),None);
  assert_eq!(slider(12.0,8.0,32.0,1.0,"KeyA"),None);
 }
 #[test] fn scrollbar_axis_bounds_and_pages(){
  assert_eq!(scroll(0.0,1000.0,250.0,false,"ArrowDown"),Some(48.0));
  assert_eq!(scroll(0.0,1000.0,250.0,false,"ArrowUp"),Some(0.0));
  assert_eq!(scroll(0.0,1000.0,250.0,true,"ArrowDown"),None);
  assert_eq!(scroll(0.0,1000.0,250.0,true,"ArrowRight"),Some(48.0));
  assert_eq!(scroll(700.0,1000.0,250.0,false,"PageDown"),Some(750.0));
  assert_eq!(scroll(0.0,100.0,250.0,false,"End"),Some(0.0));
 }
}
