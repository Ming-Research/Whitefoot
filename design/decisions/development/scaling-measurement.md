# Scaling measurement

How the runtime's scaling with core count is measured: the references the Whitefoot server is compared with.

### decision echo-scaling-is-compared-with-two-c-servers
The echo server's scaling with cores is compared with an io_uring and an epoll server in C, at the same core counts and the same client, with no Rust or C++ language-runtime server.
- rejected: an agent proposed adding a Rust server on a multithreaded runtime and a C++ server on asio as references; the owner rejected it, because it needs two more servers, Rust and asio on the runner and their echo correctness checked, and a second choice of how to write them.
