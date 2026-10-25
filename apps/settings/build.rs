use embed_manifest::manifest::DpiAwareness;
use embed_manifest::{embed_manifest, new_manifest};

fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest(
            new_manifest("NewWinSettings")
                .dpi_awareness(DpiAwareness::PerMonitorV2Only)
                .long_path_aware(embed_manifest::manifest::Setting::Enabled),
        )
        .expect("unable to embed manifest");
    }
    embed_resource::compile("newwin.rc", embed_resource::NONE).manifest_required().expect("unable to embed the icon");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=newwin.rc");
    println!("cargo:rerun-if-changed=../../assets/newwin.ico");
}
