// Embute o ícone do app (o mesmo `icon.ico` do projeto Delphi) como recurso
// `MAINICON`: a classe da janela, a bandeja e a hibernação o carregam por nome,
// igual ao `LoadIcon(HInstance, 'MAINICON')` do Delphi.
fn main() {
    println!("cargo:rerun-if-changed=noobs.rc");
    println!("cargo:rerun-if-changed=../icon.ico");
    embed_resource::compile("noobs.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("falha ao compilar noobs.rc (rc.exe do Windows SDK)");
}
