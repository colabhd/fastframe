# fastframe-graphics

Open an egui window on the graphics a computer has, and say why when it has
none.

eframe's glow renderer needs OpenGL 2 or later. A Windows computer without
its graphics driver (the Microsoft Basic Display Adapter, many virtual
machines, some Remote Desktop sessions) offers only OpenGL 1.1, so
`eframe::run_native` returns `egui_glow requires opengl 2.0+` before the
window draws anything. Started from Explorer, an app has no console, so it
seemed to quit without a word (Solco's Windows first-run test).

## Usage

```rust
fastframe_graphics::Graphics::new("Solco")
    .help("https://support.getsolco.com/graphics-driver")
    .run_native(native_options, || {
        let database = database.clone();
        Box::new(move |cc| Ok(Box::new(App::new(cc, database))))
    })?;
```

`run_native` takes the window's options and app creator as functions,
because a try that fails consumes both. It replaces the renderer in the
options:

1. It opens the window with glow, the renderer every app has drawn with.
2. When glow cannot draw it and the `wgpu` feature is on, it opens it again
   with wgpu. On Windows that draws through Direct3D 12 and, without a vendor
   driver, through WARP, Windows' own software renderer.
3. When no renderer can draw, it shows a native dialog that names the cause
   and what to do, with a link to the app's help page, and returns the error.

Another renderer is tried only when the window never started (the app
creator did not run) and the error is a graphics one
(`is_graphics_failure`: glutin found no configuration or context, OpenGL is
older than 2, or wgpu found no adapter or device). A failure after the window
opened is returned as it is, and a panic after it opened goes on unchanged.
Later windows in the same process use the renderer that worked, and
`fastframe_graphics::renderer()` names it, for error reports.

The dialog is a message box on Windows (`MessageBoxW`, which needs no window,
console or driver), an `NSAlert` on macOS, and zenity or kdialog on Linux,
whichever is installed. `Graphics::explain` shows it on its own, for an app
that opens its window another way.

With fastframe-shell, clone the lease into each app creator; only one of them
takes the app:

```rust
shell.run(|lease| {
    graphics.run_native(native_options, || {
        let lease = lease.clone();
        Box::new(move |cc| Ok(Box::new(Window { app: lease.take(&cc.egui_ctx) })))
    })
})
```

## The wgpu fallback is the app's choice

```toml
[target.'cfg(windows)'.dependencies]
fastframe-graphics = { git = "https://github.com/crmne/fastframe", tag = "v0.15.0", features = ["wgpu"] }
```

It adds wgpu to the build: about 7 MB on a Windows release binary. On the
Microsoft Basic Display Adapter (a Windows 11 virtual machine without 3D
acceleration), glow failed with `egui_glow requires opengl 2.0+` and wgpu
drew the window with Direct3D 12 on the Microsoft Basic Render Driver. WARP
draws on the processor, so animation costs CPU time; the app runs, slowly
rather than not at all.

- With both renderers compiled in, eframe's own default becomes wgpu.
  `run_native` always sets glow first, so the app keeps its renderer on
  every computer that has OpenGL 2.
- Code that draws with glow directly (paint callbacks, `cc.gl`) needs a wgpu
  path or must check `renderer()`. `eframe::App::on_exit` receives `None` for
  the GL context under wgpu.
- wgpu panics when it cannot configure the window's surface; Direct3D 12 did
  in a Windows session without a desktop (over SSH). Before the window
  started, `run_native` catches that panic and treats it as one more renderer
  that cannot draw, so the dialog still shows. An app's panic hook still
  sees it first.

## Checking it

`cargo run -p fastframe-graphics --example dialog` shows the dialog.
`cargo run -p fastframe-graphics --example window [--features wgpu] -- --report renderer.txt`
opens a window, writes the renderer that drew it and closes; on a computer
without OpenGL 2 it shows the fallback or the dialog.
