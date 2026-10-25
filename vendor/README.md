# Patched dependencies

Copies of published crates with a local fix, used instead of the published
version through `[patch.crates-io]` in the workspace's `Cargo.toml`. Each is
the published crate as downloaded, changed only where marked
`LED Badge Studio patch`, so `diff` against the published version shows the
whole change. Remove a copy (and its `[patch.crates-io]` line) once the fix
is released upstream; when updating the dependency, copy the new version and
apply the patch again.

## gpui-pre-wgpu 0.3.9

GPUI's renderer (Zed's `gpui_wgpu`, Apache-2.0, see `LICENSE-APACHE`).

**Fix** (`src/wgpu_context.rs`): on desktops, OpenGL adapters aren't asked
for compute-shader limits (GPUI has no compute shaders), and OpenGL without
the storage buffers GPUI's usual shaders read in the vertex stage (wgpu's
`DownlevelFlags::VERTEX_STORAGE`) draws with its WebGL shaders instead, as it already does in browsers (`try_adapter_with_surface`,
`uses_webgl_instance_data`). Before, `wgpu::Limits::downlevel_defaults()`
asked for compute limits that OpenGL through virgl can't meet. That's the
virtual GPU of Linux VMs under QEMU and macOS virtualization, where Vulkan
isn't available: the device failed to start and GPUI fell back to Mesa's
software renderer (llvmpipe), rendering on the CPU.
