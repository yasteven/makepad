// Tests the shared fork's actual helpers without building an application or WASM.
import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync, existsSync, mkdtempSync, writeFileSync, rmSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {spawnSync} from 'node:child_process';
const root = fileURLToPath(new URL('../../../../', import.meta.url));
const read = p => readFileSync(join(root, p), 'utf8');
const widgets = ['widgets/src', 'widgets/core/src'].filter(p => existsSync(join(root,p,'input_keyboard.rs')));
assert.equal(widgets.length, 1);

function rust(t, name, text, expected) {
  if (process.env.MAKEPAD_COMPLETION_SKIP_RUSTC === '1') {
    t.skip('Rust compiler unavailable in preparation environment; required for full validation');
    return;
  }
  const dir = mkdtempSync(join(tmpdir(), 'makepad-unified-'));
  try {
    const source = join(dir, name+'.rs'), binary = join(dir, name);
    writeFileSync(source, text);
    const build = spawnSync(process.env.RUSTC || 'rustc', ['--edition=2021','--test',source,'-o',binary], {encoding:'utf8',timeout:120000});
    assert.equal(build.status,0,`${build.error || ''}\n${build.stdout}\n${build.stderr}`);
    const run = spawnSync(binary,['--test-threads=1'],{encoding:'utf8',timeout:120000});
    assert.equal(run.status,0,`${run.error || ''}\n${run.stdout}\n${run.stderr}`);
    assert.match(run.stdout,new RegExp(`test result: ok\\. ${expected} passed; 0 failed; 0 ignored`));
  } finally {rmSync(dir,{recursive:true,force:true});}
}

test('shared fork has one clock module, keyboard ownership and WASM layout guard', () => {
  assert.equal(read('platform/src/lib.rs').split('pub mod browser_time;').length-1,1);
  assert.equal(read(widgets[0]+'/lib.rs').split('mod input_keyboard;').length-1,1);
  assert.match(read(widgets[0]+'/slider.rs'),/SliderAction::TextSlide/);
  assert.match(read(widgets[0]+'/slider.rs'),/input_keyboard::quantized/);
  assert.match(read(widgets[0]+'/scroll_bar.rs'),/input_keyboard::scroll/);
  assert.match(read('draw/src/text/fonts.rs'),/f32::INFINITY/);
  const vars = read('platform/src/draw_vars.rs');
  assert.match(vars,/DrawVars instance array must end exactly at the struct boundary/);
  assert.ok(vars.includes('instance_align_pad') || vars.includes('dyn_instances_pad'));
  assert.match(read('fork_source.txt'),/^format=makepad-fork-v1\n/);
});

test('shared keyboard helper preserves fractional steps, bounds and axis selection', t => {
  rust(t,'keyboard',read(widgets[0]+'/input_keyboard.rs'),2);
});

test('actual browser clock implementation handles elapsed time, epochs and overflow', t => {
  // Compile the browser branch on the host, replacing only its target gates.
  // This exercises the implementation; it is not a WASM integration build.
  const shim=read('platform/src/browser_time.rs')
    .replaceAll('#[cfg(not(target_arch = "wasm32"))]','#[cfg(not(test))]')
    .replaceAll('#[cfg(target_arch = "wasm32")]','#[cfg(test)]');
  rust(t,'clock',`mod clock {\n${shim}\n}
use std::sync::atomic::{AtomicU64, Ordering};
static MONO: AtomicU64 = AtomicU64::new(0);
static WALL: AtomicU64 = AtomicU64::new(0);
struct Cx;
impl Cx {
 fn monotonic_now() -> f64 { f64::from_bits(MONO.load(Ordering::Relaxed)) }
 fn time_now() -> f64 { f64::from_bits(WALL.load(Ordering::Relaxed)) }
}
#[test] fn monotonic_arithmetic() {
 use clock::*;
 MONO.store(10.0f64.to_bits(),Ordering::Relaxed); let a=Instant::now();
 MONO.store(12.5f64.to_bits(),Ordering::Relaxed); let b=Instant::now();
 assert_eq!(a.elapsed(),Duration::from_millis(2500));
 assert_eq!(b-a,Duration::from_millis(2500));
 assert_eq!(a.checked_duration_since(b),None);
 assert_eq!(a.saturating_duration_since(b),Duration::ZERO);
 assert_eq!(a+Duration::from_millis(2500),b);
 assert!(a.checked_add(Duration::MAX).is_none());
}
#[test] fn wall_clock_and_invalid_inputs() {
 use clock::*;
 WALL.store(100.0f64.to_bits(),Ordering::Relaxed);let a=SystemTime::now();
 assert_eq!(a.duration_since(UNIX_EPOCH).unwrap(),Duration::from_secs(100));
 WALL.store(99.0f64.to_bits(),Ordering::Relaxed);
 assert_eq!(SystemTime::now().duration_since(a).unwrap_err().duration(),Duration::from_secs(1));
 assert!(a.checked_sub(Duration::from_secs(101)).is_none());
 MONO.store(f64::NAN.to_bits(),Ordering::Relaxed);let zero=Instant::now();
 MONO.store((-1.0f64).to_bits(),Ordering::Relaxed);assert_eq!(zero,Instant::now());
}
`,2);
});
