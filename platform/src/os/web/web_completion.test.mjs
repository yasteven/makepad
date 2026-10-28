// Focused browser-completion regression. Run with Node and rustc; no Cargo,
// application build, WASM toolchain, network or running broker is required.
// The native harness compiles the actual allocation ledger, submission serials,
// repaint/poll methods and completion handler extracted from this source tree.
// Draw encoding and GPU acknowledgements are mocked, not a browser soak test.
import assert from "node:assert/strict";
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const read = path => readFileSync(new URL(path, import.meta.url), "utf8");
const web = read("./web.rs");
const renderer = read("./web_gl.rs");
const ledger = read("../../retained_instances.rs");
const textures = read("../../texture.rs");
const api = read("../../cx_api.rs");
const js = read("./web_gl.js")
  .replace(/^import \{[\s\S]*?\} from "\.\/web\.js";\n/, "")
  .replace(/^export /gm, "");

function between(source, start, end) {
  const a = source.indexOf(start);
  assert.ok(a >= 0, `Missing source anchor: ${start}`);
  const b = source.indexOf(end, a + start.length);
  assert.ok(b > a, `Missing source end: ${end}`);
  return source.slice(a, b);
}

// All extracted blocks below contain balanced braces, including comments.
// Fail at the source boundary if the backend is reorganized.
function item(source, signature) {
  const a = source.indexOf(signature);
  assert.ok(a >= 0, `Missing Rust item: ${signature}`);
  let depth = 0;
  for (let i = source.indexOf("{", a); i < source.length; i++) {
    if (source[i] === "{") depth++;
    if (source[i] === "}" && --depth === 0) return source.slice(a, i + 1);
  }
  assert.fail(`Unclosed Rust item: ${signature}`);
}

const repaint = item(web, "pub fn handle_repaint(");
const poll = item(renderer, "pub(crate) fn poll_texture_lifetimes(");
const retirement = between(renderer,
  "pub(crate) fn retire_webgl_resources(&mut self) {",
  "        let mut array_buffer_ids");
const schedule = between(web, "if self.any_passes_dirty()",
  "            self.os.from_wasm(FromWasmRequestAnimationFrame");
const scheduleCondition = schedule.slice(3, schedule.lastIndexOf("{")).trim();
const completionArm = between(web, "live_id!(ToWasmGpuCompletion) => {",
  "                live_id!(ToWasmRenderTextureCapture)");
const completionBody = completionArm.slice(completionArm.indexOf("let serial="),
  completionArm.lastIndexOf("}"));

test("animation-frame wiring services GPU work after encoding and while idle", () => {
  assert.ok(repaint.lastIndexOf("self.poll_texture_lifetimes();") >
    repaint.lastIndexOf("self.draw_pass_to_texture"));
  assert.match(scheduleCondition,
    /self\.frame_submission_serial\(\)\s*>\s*self\.frame_completed_serial\(\)/);
  // A failed fence's synchronous callback must not immediately re-arm itself.
  assert.ok(!completionBody.includes("poll_texture_lifetimes"));
  assert.ok(!retirement.includes("poll_texture_lifetimes"));
});

function gpuHarness(statuses) {
  let now = 0, nextId = 0;
  const timers = [], live = new Set(), replies = [];
  const gl = {
    SYNC_GPU_COMMANDS_COMPLETE: 1, TIMEOUT_EXPIRED: 2, WAIT_FAILED: 3,
    CONDITION_SATISFIED: 4, ALREADY_SIGNALED: 5,
    isContextLost: () => false,
    fenceSync() { const fence = ++nextId; live.add(fence); return fence; },
    deleteSync(fence) { assert.ok(live.delete(fence)); },
    flush() {},
    clientWaitSync() { return statuses.length > 1 ? statuses.shift() : statuses[0]; },
  };
  const WasmWebGL = new Function("WasmWebBrowser", "setTimeout", "performance",
    `${js}\nreturn WasmWebGL;`)(class {},
    (fn, ms) => { timers.push({ fn, ms }); return timers.length; },
    { now: () => now });
  const app = Object.create(WasmWebGL.prototype);
  Object.assign(app, {
    gl, wasm: {}, webgl_context_lost: false,
    to_wasm: { ToWasmGpuCompletion: value => replies.push(value) },
    do_wasm_pump() {},
  });
  return { app, gl, replies, live, timers,
    next(delta = 16) { now += delta; assert.ok(timers.length); timers.shift().fn(); } };
}

test("GPU fence releases once only after a signaled completion", () => {
  const h = gpuHarness([2, 2, 4]);
  h.app.FromWasmPollGpuCompletion({ serial_lo: 123, serial_hi: 2 });
  assert.equal(h.replies.length, 0);
  assert.equal(h.live.size, 1);
  h.next(); assert.equal(h.replies.length, 0);
  h.next();
  assert.deepEqual(h.replies, [{ serial_lo: 123, serial_hi: 2, success: true }]);
  assert.equal(h.live.size, 0);
  assert.equal(h.timers.length, 0);
});

test("failed or timed-out fences do not fabricate completion", () => {
  for (const status of [2, 3]) {
    const h = gpuHarness([status]);
    h.app.FromWasmPollGpuCompletion({ serial_lo: 9, serial_hi: 0 });
    if (status === 2) h.next(10001);
    assert.deepEqual(h.replies, [{ serial_lo: 9, serial_hi: 0, success: false }]);
    assert.equal(h.live.size, 0);
    assert.equal(h.timers.length, 0);
  }
});

test("destroyed app drops the pending fence without a WASM callback", () => {
  const h = gpuHarness([2]);
  h.app.FromWasmPollGpuCompletion({ serial_lo: 9, serial_hi: 0 });
  h.app.wasm = null;
  h.next();
  assert.equal(h.replies.length, 0);
  assert.equal(h.live.size, 0);
});

const harness = String.raw`
#![allow(dead_code, unexpected_cfgs)]
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
${between(ledger, "#[derive(Debug)]\npub struct RetainedAllocationBudget", "/// Retained storage gets")}
${item(ledger, "impl RetainedAllocationBudget {")}
#[derive(Default)]
${item(textures, "pub(crate) struct FrameSerials {")}
${item(textures, "impl FrameSerials {")}

#[derive(Clone)]
enum CxDrawPassParent { Xr, Window(usize), DrawPass(usize), None }
struct Pass { parent: CxDrawPassParent, paint_dirty: bool }
impl Pass { fn set_time(&mut self, _: f32, _: u64) {} }
struct Windows;
impl Windows { fn current_id_zero(&self) -> Option<usize> { Some(0) } }
struct RetiredTexture { serial: u64 }
#[derive(Default)]
struct TextureState { serials: FrameSerials, retired: Vec<RetiredTexture> }
struct DrawLists { allocations: RetainedAllocationBudget }
struct Publications;
impl Publications { fn retire_without_backing(&mut self) {} }
struct FromWasmPollGpuCompletion { serial_lo: u32, serial_hi: u32 }
#[derive(Default)]
struct Os { completion_pending: u64, requests: Vec<u64> }
impl Os {
    fn from_wasm(&mut self, m: FromWasmPollGpuCompletion) {
        self.requests.push(m.serial_lo as u64 | ((m.serial_hi as u64) << 32));
    }
}
struct Completion { serial_lo: u32, serial_hi: u32, success: bool }
enum Event { Signal }
const ITEMS: usize = 200;
const BYTES: usize = 256;
const LIMIT: usize = ITEMS * BYTES * 8;
struct Cx {
    textures: ((), TextureState), draw_lists: ((), DrawLists),
    publications: Publications, os: Os, passes: Vec<Pass>, windows: Windows,
    repaint_id: u64, generation: u64, demo_time_repaint: bool,
    new_next_frames: Vec<()>, paint: bool, draws: usize, signals: usize,
    charges: Vec<Option<RetainedAllocation>>,
}
impl Cx {
    fn new() -> Self {
        Self { textures: ((), TextureState::default()),
            draw_lists: ((), DrawLists { allocations: RetainedAllocationBudget::new(LIMIT) }),
            publications: Publications, os: Os::default(),
            passes: vec![Pass { parent: CxDrawPassParent::Window(0), paint_dirty: false }],
            windows: Windows, repaint_id: 0, generation: 0, demo_time_repaint: false,
            new_next_frames: vec![], paint: false, draws: 0, signals: 0,
            charges: vec![None; ITEMS] }
    }
    fn compute_pass_repaint_order(&mut self, out: &mut Vec<usize>) {
        if self.paint { out.push(0); }
        self.demo_time_repaint = false;
    }
    fn next_uniform_gen(&mut self) -> u64 { self.generation += 1; self.generation }
    fn draw_pass_to_canvas(&mut self, _: usize) {
        // Model ordinary replacement uploads, with actual production accounting.
        let next = self.frame_submission_serial() + 1;
        for slot in &mut self.charges {
            if let Some(charge) = self.draw_lists.1.allocations.reserve(BYTES) {
                charge.submitted(next);
                *slot = Some(charge);
                self.draws += 1;
            } else { self.demo_time_repaint = true; }
        }
        self.textures.1.serials.submit();
    }
    fn draw_pass_to_texture(&mut self, id: usize) { self.draw_pass_to_canvas(id); }
    fn any_passes_dirty(&self) -> bool { false }
    fn need_redrawing(&self) -> bool { false }
    fn has_pending_webgl_resource_retirements(&self) -> bool {
        self.draw_lists.1.allocations.has_pending_retirements()
    }
    fn call_event_handler(&mut self, _: &Event) { self.signals += 1; }
    ${item(api, "pub fn frame_submission_serial(")}
    ${item(api, "pub fn frame_completed_serial(")}
    ${repaint}
    ${poll}
    ${retirement}
    }
    fn requests_frame(&self) -> bool { ${scheduleCondition} }
    fn acknowledge(&mut self, serial: u64, success: bool) {
        let tw = Completion { serial_lo: serial as u32, serial_hi: (serial >> 32) as u32, success };
        ${completionBody}
    }
    fn step(&mut self, paint: bool) {
        self.paint = paint;
        self.retire_webgl_resources();
        self.handle_repaint(0.0);
        self.retire_webgl_resources();
        // Simulate the existing worker that drops collected account metadata.
        while self.draw_lists.1.allocations.take_metadata_disposal().is_some() {}
    }
}
#[test]
fn repeated_uploads_stay_bounded_with_delayed_gpu_completions() {
    let mut cx = Cx::new();
    for frame in 0..5000 {
        if frame % 3 == 0 && cx.os.completion_pending != 0 {
            cx.acknowledge(cx.os.completion_pending, true);
        }
        let before = cx.draws;
        cx.step(true);
        assert_eq!(cx.draws - before, ITEMS, "draws stopped at frame {frame}");
        assert!(cx.draw_lists.1.allocations.bytes() <= LIMIT);
        assert!(cx.draw_lists.1.allocations.record_count() <= LIMIT / BYTES);
    }
    assert!(cx.frame_completed_serial() > 0);
    assert_eq!(cx.draw_lists.1.allocations.refusals(), 0);
}
#[test]
fn frozen_gpu_refuses_uploads_and_real_completion_recovers_them() {
    let mut cx = Cx::new();
    for _ in 0..30 { cx.step(true); }
    assert!(cx.draw_lists.1.allocations.refusals() > 0);
    assert_eq!(cx.frame_completed_serial(), 0);
    assert_eq!(cx.os.requests.len(), 1, "only one completion fence may be in flight");
    let draws = cx.draws;
    cx.step(true);
    assert_eq!(cx.draws, draws);
    for _ in 0..30 {
        cx.acknowledge(cx.os.completion_pending, true);
        cx.step(true);
    }
    let before = cx.draws;
    cx.acknowledge(cx.os.completion_pending, true);
    cx.step(true);
    assert_eq!(cx.draws - before, ITEMS);
    assert!(cx.draw_lists.1.allocations.bytes() <= LIMIT);
}
#[test]
fn idle_frame_catches_up_and_stops_requesting_frames() {
    let mut cx = Cx::new();
    cx.step(true); cx.step(true);
    assert_eq!(cx.os.requests, vec![1]);
    cx.acknowledge(1, true);
    assert!(cx.requests_frame());
    cx.step(false);
    assert_eq!(cx.os.requests, vec![1, 2]);
    cx.acknowledge(2, true);
    for _ in 0..10 { cx.step(false); }
    assert_eq!(cx.frame_submission_serial(), 2);
    assert_eq!(cx.frame_completed_serial(), 2);
    assert!(!cx.requests_frame());
    assert_eq!(cx.draw_lists.1.allocations.bytes(), ITEMS * BYTES);
}
#[test]
fn failed_fence_retries_on_frame_not_completion_callback() {
    let mut cx = Cx::new(); cx.step(true);
    cx.acknowledge(1, false);
    assert_eq!(cx.frame_completed_serial(), 0);
    assert_eq!(cx.os.completion_pending, 0);
    assert_eq!(cx.os.requests, vec![1]);
    assert!(cx.requests_frame());
    cx.step(false);
    assert_eq!(cx.os.requests, vec![1, 1]);
    cx.acknowledge(999, true); // unrelated/stale acknowledgment is ignored
    assert_eq!(cx.frame_completed_serial(), 0);
    assert_eq!(cx.os.completion_pending, 1);
    cx.acknowledge(1, true); cx.step(false);
    assert!(!cx.requests_frame());
}
#[test]
fn maintenance_collects_a_bounded_backlog_once_per_frame() {
    let mut cx = Cx::new();
    for _ in 0..1200 {
        let charge = cx.draw_lists.1.allocations.reserve(BYTES).unwrap();
        charge.submitted(1);
    }
    cx.textures.1.serials.submit();
    cx.textures.1.serials.complete(1);
    cx.retire_webgl_resources();
    let remaining = cx.draw_lists.1.allocations.record_count();
    assert_eq!(1200 - remaining, 64 * 9);
    cx.retire_webgl_resources();
    assert_eq!(cx.draw_lists.1.allocations.record_count(), remaining);
    for _ in 0..10 { cx.step(false); }
    assert_eq!(cx.draw_lists.1.allocations.bytes(), 0);
}
`;

test("production Rust ledger and browser frame service survive sustained churn", t => {
  const compiler = process.env.RUSTC || "rustc";
  const version = spawnSync(compiler, ["--version"], { encoding: "utf8" });
  if (version.error && process.env.MAKEPAD_COMPLETION_SKIP_RUSTC === "1") {
    t.skip("rustc unavailable in preparation environment; native regression still required");
    return;
  }
  assert.equal(version.status, 0, version.error?.message || version.stderr);
  const dir = mkdtempSync(join(tmpdir(), "makepad-web-completion-"));
  try {
    const source = join(dir, "regression.rs"), binary = join(dir, "regression");
    writeFileSync(source, harness);
    const build = spawnSync(compiler,
      ["--edition=2021", "--test", source, "-o", binary], { encoding: "utf8" });
    assert.equal(build.status, 0, build.error?.message || build.stderr);
    const run = spawnSync(binary, ["--test-threads=1"], { encoding: "utf8" });
    assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
    assert.match(run.stdout, /5 passed; 0 failed/);
    t.diagnostic(run.stdout.trim());
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// Optional output for source-level review, without invoking a compiler.
if (process.env.MAKEPAD_COMPLETION_HARNESS_OUT) {
  writeFileSync(process.env.MAKEPAD_COMPLETION_HARNESS_OUT, harness);
}
