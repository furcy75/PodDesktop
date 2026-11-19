// Script de build Tauri v2 — génère les ressources Windows (icône, manifeste) et le contexte applicatif.
fn main() {
    // Force Cargo à ré-embarquer les assets frontend dès que `dist-web/` est reconstruit par Vite.
    println!("cargo:rerun-if-changed=../dist-web");
    println!("cargo:rerun-if-changed=../dist-web/index.html");
    tauri_build::build()
}
