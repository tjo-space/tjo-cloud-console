use kube::CustomResourceExt;
fn main() {
    print!(
        "{}",
        serde_yaml::to_string(&console::Document::crd()).unwrap()
    )
}
