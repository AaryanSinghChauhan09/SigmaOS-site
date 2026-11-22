//! SigmaOS component binary: site

use sigmaos_site::Component;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Component::new().operate()?;
    println!("sigmaos-site: ok");
    Ok(())
}
