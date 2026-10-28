pub fn get_home_directory() -> String {
    std::env::var("HOME")
        .expect("HOME environment variable not set")
}
