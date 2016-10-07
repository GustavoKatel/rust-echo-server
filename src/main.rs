extern crate mio;

use std::str;
use std::vec;
use std::time::Duration;
use std::collections::HashMap;
use std::io::{Write, Read};

use mio::*;
use mio::tcp::{TcpListener, TcpStream};

const MAX_CONN : usize = 1024;

const MAX_BUFFER : usize = 1024 * 5;
const MAX_READ : usize = 1024;

struct Connection {
    stream : TcpStream,
    buffer : Vec<u8>,
    offset : usize,
}

impl Connection {
    fn new(stream : TcpStream) -> Connection {
        Connection{stream: stream, buffer: Vec::new(), offset: 0}
    }

    fn read(&mut self) -> Result<usize, std::io::Error> {

        let mut buff : [u8; MAX_READ] = [0; MAX_READ];

        match self.stream.read(&mut buff) {
            Ok(bytes_read) => {

                let s = match str::from_utf8(&buff) {
                    Ok(v) => v,
                    Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
                };

                self.buffer.extend(buff.iter().cloned());

                Ok(bytes_read)
            }
            Err(e) => Err(e)
        }
    }

    fn write(&mut self) -> Result<(), std::io::Error> {

        match self.stream.write_all(self.buffer.as_slice()) {
            Ok(_) => {
                 self.buffer.clear();
                 Ok(())
            },
            Err(e) => Err(e)
        }

    }

}

fn main() {

    let mut next_token : usize = 0;

    let addr = "[::1]:4040".parse().unwrap();

    const SERVER_TOKEN: Token = Token(0);
    let server = TcpListener::bind(&addr).unwrap();

    // create a new poll
    let poll = Poll::new().unwrap();

    // Start listening for incoming connections
    // register with level-triggered opt
    poll.register(&server, SERVER_TOKEN, Ready::readable() | Ready::hup(),
              PollOpt::level()).unwrap();

    // Create storage for events
    let mut events = Events::with_capacity(1024);

    let mut connections : HashMap<Token, Connection>  = HashMap::new();

    loop {
        // poll with 5s timeout
        poll.poll(&mut events, Some(Duration::new(5, 0))).unwrap();

        for event in events.iter() {
            match event.token() {
                SERVER_TOKEN => {
                    // Accept and drop the socket immediately, this will close
                    // the socket and notify the client of the EOF.
                    match server.accept() {
                        Ok((stream, addr)) => {

                            next_token = (next_token + 1) % MAX_CONN;
                            if next_token == 0 {
                                next_token += 1; // skips server
                            }

                            let new_token = Token(next_token);

                            connections.insert(new_token, Connection::new(stream));

                            // register the new socket
                            match connections.get(&new_token) {
                                Some(connection) => poll.register(&connection.stream, new_token, Ready::readable() | Ready::hup(), PollOpt::level()).unwrap(),
                                _ => panic!("Could not register the new connection"),
                            }

                        }
                        Err(e) => {
                            panic!("Error: {}", e);
                        }
                    }
                }
                token => {
                    match connections.get_mut(&token) {
                        Some(connection) => {

                            // we have data to read
                            if event.kind().is_readable() {
                                match connection.read() {
                                    Ok(bytes_read) => {
                                        // we got some bytes in the buffer, register to wait for
                                        // writable events in the poll
                                        if bytes_read > 0 {
                                            poll.reregister(&connection.stream, token, Ready::writable() | Ready::hup(), PollOpt::level()).unwrap();
                                        }
                                    }
                                    Err(e) => {
                                        panic!("Error {}", e);
                                    }
                                }
                            }

                            // we are able to write
                            if event.kind().is_writable() {
                                match connection.write() {
                                    Ok(_) => {
                                        // we sucessfully wrote data back to the stream
                                        // reregister in the poll to read more data
                                        poll.reregister(&connection.stream, token, Ready::readable() | Ready::hup(), PollOpt::level()).unwrap();
                                    }
                                    Err(e) => {
                                        panic!("Error {}", e);
                                    }
                                }
                            }

                            // we got a hup
                            if event.kind().is_hup() {
                                poll.deregister(&connection.stream).unwrap();

                            }

                        },
                        _ => panic!("Could not get the registered connection"),
                    }
                },
            }
        }
    }

}
