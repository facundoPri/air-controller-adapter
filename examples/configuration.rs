//! Print a valid default config without opening devices or executing actions.
fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&ready_for_air::studio::Config::default()).unwrap()
    );
}
