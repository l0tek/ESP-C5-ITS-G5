//! Generate a synthetic log for testing the Android import without a radio source.
fn main() {
    for frame in cits_map::demo::frames() {
        print!("CITS:");
        for byte in frame {
            print!("{byte:02x}");
        }
        println!();
    }
}
