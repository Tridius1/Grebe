
use winres;

fn main() {
    // Package exe with icon
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        
        res.set_icon("grebe_icon.ico")
        .set("OriginalFilename", "Grebe.exe")
        .set("ProductName", "Grebe")
        .set("CompanyName", "Tristan Swanson");
        
        if let Err(e) = res.compile() {
            eprintln!("Failed to compile resources: {}", e);
            std::process::exit(1);
        }
    }
}