fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(err) = res.compile() {
            // The window/taskbar icon still works without an embedded resource;
            // only the file icon in Explorer would be missing.
            println!("cargo:warning=exe icon resource skipped: {err}");
        }
    }
}
