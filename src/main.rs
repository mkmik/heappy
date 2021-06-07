use croaring::Bitmap;
use pprof::protos::Message;
use std::io::Write;

#[derive(Debug)]
struct Foo {
    i: i32,
    v: Vec<u8>,
}

fn push(v: &mut Vec<u8>, i: u8) {
    v.push(i);
}

impl Foo {
    fn new(i: i32) -> Self {
        let mut v = Vec::with_capacity(1000);
        for i in 0..1000 {
            push(&mut v, i as u8);
        }
        Self { i, v }
    }

    fn foo(&self) {
        println!("{:?}", self.v.iter().fold(0u8, |a, b| a.wrapping_add(*b)));
    }
}

fn foo(fs: &[Foo]) {
    for f in fs {
        f.foo();
    }
}

fn main() {
    let heap_profiler_guard = ruback::HeapProfilerGuard::new();

    let mut rb1 = Bitmap::create();
    rb1.add(1);
    rb1.add(2);
    rb1.add(3);
    rb1.add(4);
    rb1.add(5);
    rb1.add(100);
    rb1.add(1000);
    println!("optimizing");
    rb1.run_optimize();
    println!("done");
    assert!(rb1.contains(3));

    // loop {
    let v = vec![1, 2, 3, 4];
    let m: Vec<Foo> = v.into_iter().map(Foo::new).collect();

    foo(&m);
    //}

    let report = heap_profiler_guard.report();

    let filename = "/tmp/memflame.svg";
    println!("Writing to {}", filename);
    let mut file = std::fs::File::create(filename).unwrap();
    report.flamegraph(&mut file);

    let proto = report.pprof();

    let mut buf = vec![];
    proto.encode(&mut buf).unwrap();
    let filename = "/tmp/memflame.pb";
    println!("Writing to {}", filename);
    let mut file = std::fs::File::create(filename).unwrap();
    file.write_all(&buf).unwrap();
}
