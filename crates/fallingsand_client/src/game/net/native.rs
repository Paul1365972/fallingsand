use super::{ConnectTarget, Net, Session};
use bevy::log::error;
use fallingsand_net::Connection;

const DIAL_TIMEOUT_SECS: f32 = 15.0;

pub(super) struct Dialing {
    receiver: std::sync::Mutex<std::sync::mpsc::Receiver<Result<Box<dyn Connection>, String>>>,
    elapsed: f32,
}

pub(super) fn poll_dial(net: &mut Net, dt: f32) -> bool {
    let Some(dialing) = net.dialing.as_mut() else {
        return false;
    };
    dialing.elapsed += dt;
    let timed_out = dialing.elapsed > DIAL_TIMEOUT_SECS;
    let result = dialing.receiver.lock().unwrap().try_recv();
    match result {
        Ok(Ok(conn)) => {
            net.dialing = None;
            net.session = Some(Session::new(conn));
        }
        Ok(Err(err)) => {
            net.dialing = None;
            error!("failed to connect: {err}");
            net.supervisor.last_error = Some(err);
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            if timed_out {
                net.dialing = None;
                error!("connect attempt timed out after {DIAL_TIMEOUT_SECS}s");
                net.supervisor.last_error = Some("connect timed out".into());
            }
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            net.dialing = None;
            net.supervisor.last_error = Some("connect worker died".into());
        }
    }
    net.dialing.is_some()
}

pub(super) fn start_dial(net: &mut Net, target: ConnectTarget) {
    if net.runtime.is_none() {
        match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => net.runtime = Some(runtime),
            Err(err) => {
                net.supervisor.last_error = Some(err.to_string());
                return;
            }
        }
    }
    let handle = net.runtime.as_ref().unwrap().handle().clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("wt-dial".into())
        .spawn(move || {
            let result = fallingsand_net::wt_native::connect(handle, &target.url, target.cert_hash)
                .map(|conn| Box::new(conn) as Box<dyn Connection>)
                .map_err(|err| err.to_string());
            let _ = sender.send(result);
        })
        .expect("spawn dial thread");
    net.dialing = Some(Dialing {
        receiver: std::sync::Mutex::new(receiver),
        elapsed: 0.0,
    });
}
