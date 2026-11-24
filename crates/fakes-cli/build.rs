// Icono y metadatos del ejecutable en Windows.
fn main() {
    println!("cargo:rerun-if-changed=../../assets/brand/logo.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/brand/logo.ico");
        res.set("ProductName", "Fake Services");
        res.set(
            "FileDescription",
            "Fake Services: simulador local de servicios externos",
        );
        if let Err(e) = res.compile() {
            println!("cargo:warning=no se pudo incrustar el icono de Windows: {e}");
        }
    }
}
