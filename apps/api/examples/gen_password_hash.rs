// One-off: generate bcrypt hash for admin123 (cost 12) to fix login.
// Run: cargo run --example gen_password_hash
fn main() {
    let hash = bcrypt::hash("admin123", 12).expect("bcrypt hash");
    println!("{}", hash);
}
