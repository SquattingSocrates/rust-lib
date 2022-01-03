use std::marker::PhantomData;

use super::{IntoProcess, IntoProcessLink, Process};
use crate::{
    environment::{params_to_vec, Param},
    host_api,
    serializer::{Bincode, Serializer},
    LunaticError, Mailbox, Tag,
};

/// A [`Server`] is a simple process spawned from a function that can maintain a state, runs in a
/// loop and answers requests sent to it.
pub struct Server<M, R, S = Bincode>
where
    S: Serializer<(Process<R, S>, Tag, M)> + Serializer<R>,
{
    id: u64,
    serializer_type: PhantomData<(M, R, S)>,
}

impl<M, R, S> Server<M, R, S>
where
    S: Serializer<(Process<R, S>, Tag, M)> + Serializer<R>,
{
    /// Returns a globally unique process ID.
    pub fn id(&self) -> u128 {
        let mut uuid: [u8; 16] = [0; 16];
        unsafe { host_api::process::id(self.id, &mut uuid as *mut [u8; 16]) };
        u128::from_le_bytes(uuid)
    }

    pub fn request(&self, message: M) -> R {
        let tag = Tag::new();
        // Create new message.
        unsafe { host_api::message::create_data(1, 0) };
        // Create reference to self
        let this_id = unsafe { host_api::process::this() };
        let this_proc: Process<R, S> = unsafe { Process::from(this_id) };
        // During serialization resources will add themself to the message.
        S::encode(&(this_proc, tag, message)).unwrap();
        // Send it!
        unsafe { host_api::message::send(self.id) };
        // Wait on response
        unsafe { Mailbox::<R, S>::new() }.tag_receive(&[tag])
    }

    fn send_init<C>(&self, message: C)
    where
        S: Serializer<C>,
    {
        // Create new message.
        unsafe { host_api::message::create_data(1, 0) };
        // During serialization resources will add themself to the message.
        S::encode(&message).unwrap();
        // Send it!
        unsafe { host_api::message::send(self.id) };
    }
}

impl<C, M, R, S> IntoProcess<C> for Server<M, R, S>
where
    S: Serializer<C> + Serializer<(Process<R, S>, Tag, M)> + Serializer<R>,
{
    type Handler = fn(state: &mut C, request: M) -> R;

    fn spawn(state: C, handler: Self::Handler) -> Result<Server<M, R, S>, LunaticError>
    where
        Self: Sized,
    {
        spawn(false, state, handler)
    }
}

impl<C, M, R, S> IntoProcessLink<C> for Server<M, R, S>
where
    S: Serializer<C> + Serializer<(Process<R, S>, Tag, M)> + Serializer<R>,
{
    type Handler = fn(state: &mut C, request: M) -> R;

    fn spawn_link(state: C, handler: Self::Handler) -> Result<Server<M, R, S>, LunaticError>
    where
        Self: Sized,
    {
        spawn(true, state, handler)
    }
}

// `spawn` performs a low level dance that will turn a high level rust function and state into a
// correct lunatic server.
//
// For more info on how this function works, read the explanation inside super::process::spawn.
fn spawn<C, M, R, S>(
    link: bool,
    state: C,
    handler: fn(state: &mut C, request: M) -> R,
) -> Result<Server<M, R, S>, LunaticError>
where
    S: Serializer<C> + Serializer<(Process<R, S>, Tag, M)> + Serializer<R>,
{
    let (type_helper, handler) = (type_helper_wrapper::<C, M, R, S> as i32, handler as i32);

    let params = params_to_vec(&[Param::I32(type_helper), Param::I32(handler)]);
    let mut id = 0;
    let func = "_lunatic_spawn_server_by_index";
    let link = match link {
        // TODO: Do we want to be notified with the right tag once the link dies?
        //       I assume not, because only supervisors can use this information and we can't spawn
        //       this kind of processes from supervisors.
        true => 1,
        false => 0,
    };
    let result = unsafe {
        host_api::process::inherit_spawn(
            link,
            func.as_ptr(),
            func.len(),
            params.as_ptr(),
            params.len(),
            &mut id,
        )
    };
    if result == 0 {
        // If the captured variable is of size 0, we don't need to send it to another process.
        if std::mem::size_of::<C>() == 0 {
            Ok(Server {
                id,
                serializer_type: PhantomData,
            })
        } else {
            let child = Server::<M, R, S> {
                id,
                serializer_type: PhantomData,
            };
            child.send_init(state);
            Ok(child)
        }
    } else {
        Err(LunaticError::from(id))
    }
}

// Type helper
fn type_helper_wrapper<C, M, R, S>(function: usize)
where
    S: Serializer<C> + Serializer<(Process<R, S>, Tag, M)> + Serializer<R>,
{
    // If the captured variable is of size 0, don't wait on it.
    let mut state = if std::mem::size_of::<C>() == 0 {
        unsafe { std::mem::MaybeUninit::<C>::zeroed().assume_init() }
    } else {
        unsafe { Mailbox::<C, S>::new() }.receive()
    };
    let mailbox: Mailbox<(Process<R, S>, Tag, M), S> = unsafe { Mailbox::new() };
    let handler: fn(state: &mut C, request: M) -> R = unsafe { std::mem::transmute(function) };

    // Run server forever and respond to requests.
    loop {
        let (sender, tag, message) = mailbox.receive();
        let response = handler(&mut state, message);
        sender.tag_send(tag, response);
    }
}

#[export_name = "_lunatic_spawn_server_by_index"]
extern "C" fn _lunatic_spawn_server_by_index(type_helper: usize, function: usize) {
    let type_helper: fn(usize) = unsafe { std::mem::transmute(type_helper) };
    type_helper(function);
}

// Processes are equal if their UUID is equal.
impl<M, R, S> PartialEq for Server<M, R, S>
where
    S: Serializer<(Process<R, S>, Tag, M)> + Serializer<R>,
{
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl<M, R, S> std::fmt::Debug for Server<M, R, S>
where
    S: Serializer<(Process<R, S>, Tag, M)> + Serializer<R>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Process").field("uuid", &self.id()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{sleep, spawn, spawn_link};

    #[test]
    fn spawn_test() {
        let child = spawn::<Server<i32, i32>, _>(0, |state, message| {
            *state += message;
            *state
        })
        .unwrap();
        assert_eq!(child.request(1), 1);
        assert_eq!(child.request(2), 3);
        assert_eq!(child.request(3), 6);
    }

    #[test]
    fn spawn_link_test() {
        // There is no real way of testing traps for now, at least not until this is resolved:
        // https://github.com/lunatic-solutions/rust-lib/issues/8
        // A manual log output observation is necessary her to check if both processes failed.
        spawn::<Server<(), _>, _>((), |_, _| {
            let child = spawn_link::<Server<(), _>, _>((), |_, _| {
                panic!("fails");
            })
            .unwrap();
            // Trigger failure
            child.request(());
            // This process should fails too before 100ms
            sleep(100);
        })
        .unwrap();
        sleep(100);
    }
}
