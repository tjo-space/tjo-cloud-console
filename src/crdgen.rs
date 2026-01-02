use kube::CustomResourceExt;
fn main() {
    let documents = vec![
        console::resources::s3::bucket::Bucket::crd(),
        console::resources::s3::token::Token::crd(),
        console::resources::postgresql::database::Database::crd(),
        console::resources::postgresql::user::User::crd(),
    ];

    for document in documents {
        println!("---");
        print!("{}", serde_yaml::to_string(&document).unwrap());
    }
}
