# Opaque scalar types

Host values such as an exit status or a socket address: their nominal types and how a program builds them.

### decision host-scalars-are-opaque-with-constructors
A value such as ExitStatus or SocketAddress has an opaque struct type its standard library module declares, and an ordinary total function builds it.
- rejected: an agent proposed a bare integer alias for such a value; the owner rejected it, because exact nominal typing keeps an arbitrary integer from serving as that value, while a construction function makes every intended value expressible.
- rejected: an agent proposed a constructor privilege tied to native implementation; the owner rejected it, because an ordinary construction function already makes every intended value expressible, and the constructor call is refused on every opaque struct alike.