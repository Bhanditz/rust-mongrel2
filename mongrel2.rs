import std::json;
import std::map;
import result::{ok, err, chain};
import zmq::{context, socket, error};

export connect;
export connection;

type connection_t = {
    sender_id: [u8],
    sub_addr: [u8],
    pub_addr: [u8],
    reqs: zmq::socket,
    resp: zmq::socket,
};

fn connect(ctx: zmq::context,
          sender_id: [u8],
          sub_addr: [u8],
          pub_addr: [u8]) -> connection_t {
    let reqs =
        alt ctx.socket(zmq::PULL) {
          err(e) { fail e.to_str() }
          ok(reqs) { reqs }
        };
    reqs.connect(sub_addr);

    let resp =
        alt ctx.socket(zmq::PUB) {
          err(e) { fail e.to_str() }
          ok(resp) { resp }
        };
    resp.set_identity(sender_id);
    resp.connect(pub_addr);

    {
        sender_id: sender_id,
        sub_addr: sub_addr,
        pub_addr: pub_addr,
        reqs: reqs,
        resp: resp
    }
}

impl connection for connection_t {
    fn recv() -> request::t {
        alt self.reqs.recv(0) {
          err(e) { fail e.to_str() }
          ok(msg) { request::parse(msg) }
        }
    }

    fn term() {
        self.reqs.close();
        self.resp.close();
    }
}

mod request {
    type headers = map::hashmap<[u8], [u8]>;

    type t = {
        sender: [u8],
        id: int,
        path: [u8],
        headers: headers,
        body: [u8],
    };

    fn parse(msg: [u8]) -> t {
        let end = vec::len(msg);
        let (start, sender) = parse_sender(msg, 0u, end);
        let (start, id) = parse_id(msg, start, end);
        let (start, path) = parse_path(msg, start, end);
        let (headers, body) = parse_rest(msg, start, end);

        { sender: sender, id: id, path: path, headers: headers, body: body }
    }

    fn parse_sender(msg: [u8], start: uint, end: uint) -> (uint, [u8]) {
        alt vec::position_from(msg, start, end) { |c| c == ' ' as u8 } {
            none { fail "invalid sender uuid" }
            some(i) { (i + 1u, vec::slice(msg, 0u, i)) }
        }
    }

    fn parse_id(msg: [u8], start: uint, end: uint) -> (uint, int) {
        alt vec::position_from(msg, start, end) { |c| c == ' ' as u8 } {
          none { fail "invalid connection id" }
          some(i) {
            let id = vec::slice(msg, start, i);
            (i + 1u, int::parse_buf(id, 10u)) }
        }
    }

    fn parse_path(msg: [u8], start: uint, end: uint) -> (uint, [u8]) {
        alt vec::position_from(msg, start, end) { |c| c == ' ' as u8 } {
          none { fail "invalid path" }
          some(i) { (i + 1u, vec::slice(msg, start, i)) }
        }
    }

    fn parse_rest(msg: [u8], start: uint, end: uint) -> (headers, [u8]) {
        let rest = vec::slice(msg, start, end);

        let (headers, rest) = tnetstring::from_bytes(rest);
        let headers = alt headers {
          some(headers) { parse_headers(headers) }
          none { fail "empty headers" }
        };

        let (body, _) = tnetstring::from_bytes(rest);
        let body = alt body {
          some(body) { parse_body(body) }
          none { fail "empty body" }
        };

        (headers, body)
    }

    fn parse_headers(tns: tnetstring::t) -> headers {
        let headers = map::new_bytes_hash();
        alt tns {
          tnetstring::map(map) {
            map.items { |key, value|
                alt value {
                  tnetstring::string(s) { headers.insert(key, s); }
                  _ { fail "header value is not string"; }
                }
            };
          }

          // Fall back onto json if we got a string.
          tnetstring::string(s) {
            alt json::from_str(str::unsafe_from_bytes(s)) {
              some(json::dict(map)) {
                map.items { |key, value|
                    alt value {
                      json::string(v) {
                        headers.insert(str::bytes(key), str::bytes(v));
                      }
                      _ { fail "header value is not string"; }
                    }
                };
              }
              _ { fail "invalid header"; }
            }
          }

          _ { fail "invalid header"; }
        }

        headers
    }

    fn parse_body(tns: tnetstring::t) -> [u8] {
        alt tns {
          tnetstring::string(body) { body }
          _ { fail "invalid body" }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test() {
        let ctx =
            alt zmq::init(1) {
              ok(ctx) { ctx }
              err(e) { fail e.to_str() }
            };

        let connection = connection::create(ctx,
            str::bytes("F0D32575-2ABB-4957-BC8B-12DAC8AFF13A"),
            str::bytes("tcp://127.0.0.1:9998"),
            str::bytes("tcp://127.0.0.1:9999"));

        connection.term();
        ctx.term();
    }

    #[test]
    fn test_request_parse() {
        let request = request::parse(
            str::bytes("abCD-123 56 / 13:{\"foo\":\"bar\"},11:hello world,"));

        let headers = map::new_bytes_hash();
        headers.insert(str::bytes("foo"), str::bytes("bar"));

        assert request.sender == str::bytes("abCD-123");
        assert request.id == 56;
        assert request.headers == headers;
        assert request.body == str::bytes("hello world");
    }
}
