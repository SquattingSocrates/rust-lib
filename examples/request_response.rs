use lunatic::{
    process::{AbstractProcess, ProcessRef, ProcessRequest, Request, StartProcess},
    Mailbox,
};

struct Adder;
impl AbstractProcess for Adder {
    type Arg = ();
    type State = Self;

    fn init(_: ProcessRef<Self>, _: ()) -> Adder {
        Adder
    }
}
impl ProcessRequest<(i32, i32)> for Adder {
    type Response = i32;

    fn handle(_: &mut Self::State, (a, b): (i32, i32)) -> i32 {
        a + b
    }
}

#[lunatic::main]
fn main(_: Mailbox<()>) {
    let add_server = Adder::start((), None);
    assert_eq!(add_server.request((1, 1)), 2);
    assert_eq!(add_server.request((1, 2)), 3);
}
