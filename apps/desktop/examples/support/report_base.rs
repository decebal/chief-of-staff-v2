/// Where the preview serves the app's `cosreport://localhost/` pages. The
/// window's frames take only an absolute URL (`safeFrameSrc` in `ui/states.js`).
pub fn report_base(port: u16) -> String {
    format!("http://127.0.0.1:{port}/cosreport/")
}
