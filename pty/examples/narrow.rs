//! Runs a command with a fixed terminal size.
//! This is used by shelldone's doc building automation to keep
//! the --help output within a reasonable width
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::mpsc::channel;

fn main() {
    let pty_system = NativePtySystem::default();

    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let mut args = std::env::args_os().skip(1);

    let mut cmd = CommandBuilder::new(args.next().unwrap());
    cmd.args(args);

    let mut child = pair.slave.spawn_command(cmd).unwrap();

    // Release any handles owned by the slave: we don't need it now
    // that we've spawned the child.
    drop(pair.slave);

    // Read the output in another thread.
    // This is important because it is easy to encounter a situation
    // where read/write buffers fill and block either your process
    // or the spawned process.
    let (tx, rx) = channel();
    let mut reader = pair.master.try_clone_reader().unwrap();
    std::thread::spawn(move || {
        // Consume the output from the child
        let mut s = String::new();
        reader.read_to_string(&mut s).unwrap();
        tx.send(s).unwrap();
    });

    {
        // Obtain the writer.
        // When the writer is dropped, EOF will be sent to
        // the program that was spawned.
        // It is important to take the writer even if you don't
        // send anything to its stdin so that EOF can be
        // generated, otherwise you risk deadlocking yourself.
        let writer = pair.master.take_writer().unwrap();

        if cfg!(target_os = "macos") {
            // macOS quirk: the child and reader must be started and
            // allowed a brief grace period to run before we allow
            // the writer to drop. Otherwise, the data we send to
            // the kernel to trigger EOF is interleaved with the
            // data read by the reader! WTF!?
            // This appears to be a race condition for very short
            // lived processes on macOS.
            // I'd love to find a more deterministic solution to
            // this than sleeping.
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        // This example doesn't need to write anything by default, but allowing
        // runtime configuration keeps the example realistic without tripping
        // lint checks about constant conditions.
        let payload = std::env::var("PORTABLE_PTY_NARROW_WRITE")
            .ok()
            .filter(|value| !value.is_empty());

        match payload {
            Some(data) => {
                std::thread::spawn({
                    let mut writer = writer;
                    move || {
                        writer.write_all(data.as_bytes()).unwrap();
                    }
                });
            }
            None => drop(writer),
        }
    }

    // Wait for the child to complete
    eprintln!("child status: {:?}", child.wait().unwrap());

    // Take care to drop the master after our processes are
    // done, as some platforms get unhappy if it is dropped
    // sooner than that.
    drop(pair.master);

    // Now wait for the output to be read by our reader thread
    let output = rx.recv().unwrap();

    let output = output.replace("\r\n", "\n");

    print!("{output}");
}
