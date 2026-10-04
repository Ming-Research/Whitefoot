# Redis Lua codec compatibility experiment

`python3 -B research/experiments/halo-luacodecs/run.py --compiler /path/to/whitefootc --redis-source /path/to/redis-7.0.15`
builds a scratch standalone reference from Redis’s bundled Lua and all four
libraries, compiles the existing Halo end-to-end driver, and compares typed
RESP2 replies for a generated corpus, including binary strings and errors.
Use `--binary` to reuse the Halo executable and `--filter` for a small sample.
The runner never uses the network or changes either general codec package.

The new library files implement the requested Redis compatibility in the
existing VM home; remove them only if Halo no longer provides those libraries.
The runner and corpus serve this compatibility experiment and are retained
with its results; retire them when a maintained library compatibility suite
owns these observations. Python is the compiler-independent reference driver.
