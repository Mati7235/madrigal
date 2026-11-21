use std::io::BufRead;
use wayland_client::protocol::{wl_pointer::ButtonState, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1 as Mgr,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1 as Ptr,
};

#[derive(Default)]
struct S {
    seat: Option<wl_seat::WlSeat>,
    mgr: Option<Mgr>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for S {
    fn event(
        s: &mut Self,
        r: &wl_registry::WlRegistry,
        e: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = e
        {
            match interface.as_str() {
                "wl_seat" => s.seat = Some(r.bind(name, version.min(7), qh, ())),
                "zwlr_virtual_pointer_manager_v1" => {
                    s.mgr = Some(r.bind(name, version.min(2), qh, ()))
                }
                _ => {}
            }
        }
    }
}
impl Dispatch<wl_seat::WlSeat, ()> for S {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<Mgr, ()> for S {
    fn event(
        _: &mut Self,
        _: &Mgr,
        _: <Mgr as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<Ptr, ()> for S {
    fn event(
        _: &mut Self,
        _: &Ptr,
        _: <Ptr as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (w, h): (u32, u32) = (args[1].parse().unwrap(), args[2].parse().unwrap());
    let fifo = &args[3];
    let conn = Connection::connect_to_env().expect("wayland");
    let mut q = conn.new_event_queue();
    let qh = q.handle();
    conn.display().get_registry(&qh, ());
    let mut s = S::default();
    q.roundtrip(&mut s).unwrap();
    let ptr = s
        .mgr
        .as_ref()
        .expect("no virtual pointer manager")
        .create_virtual_pointer(s.seat.as_ref(), &qh, ());
    q.roundtrip(&mut s).unwrap();
    let t0 = std::time::Instant::now();
    let ms = || t0.elapsed().as_millis() as u32;
    loop {
        let f = std::fs::File::open(fifo).unwrap();
        for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
            let p: Vec<&str> = line.split_whitespace().collect();
            match p.as_slice() {
                ["move", x, y] => {
                    ptr.motion_absolute(ms(), x.parse().unwrap(), y.parse().unwrap(), w, h);
                    ptr.frame();
                }
                ["click"] | ["click", _] => {
                    let b = if p.get(1) == Some(&"right") {
                        0x111
                    } else {
                        0x110
                    };
                    ptr.button(ms(), b, ButtonState::Pressed);
                    ptr.frame();
                    conn.flush().unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(40));
                    ptr.button(ms(), b, ButtonState::Released);
                    ptr.frame();
                }
                ["scroll", dy] => {
                    ptr.axis(
                        ms(),
                        wayland_client::protocol::wl_pointer::Axis::VerticalScroll,
                        dy.parse().unwrap(),
                    );
                    ptr.frame();
                }
                ["quit"] => return,
                _ => eprintln!("? {line}"),
            }
            conn.flush().unwrap();
            q.dispatch_pending(&mut s).unwrap();
        }
    }
}
