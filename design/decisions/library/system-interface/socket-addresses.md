# Socket addresses

How a program reads a socket address's family, components and port, and the local address of a connection.

### decision socket-address-ip-returns-enum
The nonwaiting `std::net::socket_address_ip` reads only the address it is given and returns the enum `IpAddress`: `Ipv4` with four `u8` octets or `Ipv6` with eight `u16` groups, in written order.
- rejected: an agent proposed a family enum with a byte-window copy of the address; the owner rejected it, because the copy's length depends on the family, so a caller must handle a too-small window for a value whose size its pattern match already fixes.
- rejected: an agent proposed a reader of one byte at an index; the owner rejected it, because the index needs a proof, while one total pattern match gives the family and components with nothing to prove.
- rejected: an agent proposed a fixed 16-byte array; the owner rejected it, because its last twelve bytes mean nothing for IPv4.
- rejected: an agent proposed a function that formats address text; the owner rejected it, because spelling an address is the program's own choice.

### decision socket-address-port-has-one-reader
The nonwaiting `std::net::socket_address_port` reads only the address it is given and returns its `u16` port.
- rejected: an agent proposed a port reader for each family; the owner rejected it, because the port is one `u16` in both families, so it needs no pattern match and no second reader.

### decision ipv6-address-keeps-no-scope
An IPv6 socket address carries only its address and port; a scope identifier or flow label the host reports for a peer is not kept.
- rejected: an agent proposed adding the scope identifier and flow label to the address value and to `socket_address_v6`; the owner rejected it, because Firn's CLIENT LIST, the need behind these readers, prints only the address and port as Redis does.

### decision local-address-read-from-receive-half
The nonwaiting `std::net::local_address` takes a connection's receiving half and returns `Result<SocketAddress, IoError>`, whose refusal carries the host's code.
- rejected: an agent proposed a local field of AcceptedConnection filled at every accept; the owner rejected it, because every accept would pay a host call for an address most programs never read, and a connection `tcp_connect` opened would have no way to learn its end.
- rejected: an agent proposed a query on TcpConnection; the owner rejected it, because its two halves may come from different connections, so it names no single connection.
- rejected: an agent proposed a query on each half; the owner rejected it, because both halves of one connection report the same endpoint, so a second function would be a second spelling of one fact.
- rejected: an agent proposed an infallible `local_address`; the owner rejected it, because the host may refuse the query, for example for lack of resources, and an invented address would hide that refusal.