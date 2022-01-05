use lunatic::{Config, Environment, Mailbox, Task};

#[lunatic::main]
fn main(_: Mailbox<()>) {
    // Create a new environment where processes can use maximum 17 Wasm pages of
    // memory (17 * 64KB) & 1 compute unit of instructions (~=100k CPU cycles).
    let mut config = Config::new(1_200_000, Some(1));
    // Allow all host functions
    config.allow_namespace("");
    let mut env = Environment::new(config).unwrap();
    let module = env.add_this_module().unwrap();

    // This process will fail because it uses too much memory
    module
        .spawn::<Task<()>, _>((), |_| {
            vec![0; 150_000];
        })
        .unwrap();

    // This process will fail because it uses too much compute
    module.spawn::<Task<()>, _>((), |_| loop {}).unwrap();
}
