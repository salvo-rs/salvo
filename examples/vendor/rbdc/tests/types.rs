use rbdc::Uuid;

#[test]
fn test_default() {
    let u = Uuid::default();
    println!("{}", u);
    assert_eq!(u.to_string(), "00000000-0000-0000-0000-000000000000");
}
