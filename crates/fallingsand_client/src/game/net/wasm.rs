use super::{ConnectTarget, Net, Session};

pub(super) fn poll_dial(_net: &mut Net, _dt: f32) -> bool {
    false
}

pub(super) fn start_dial(net: &mut Net, target: ConnectTarget) {
    let conn = Box::new(fallingsand_net::wt_wasm::connect(
        &target.url,
        target.cert_hash,
    ));
    net.session = Some(Session::new(conn));
}
