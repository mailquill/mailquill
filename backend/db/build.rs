// sqlx::migrate! embeds the migration files at compile time, but adding a new
// file to the directory does not by itself invalidate the crate. Without this
// rerun hint, a stale build silently ships without the newest migrations.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
