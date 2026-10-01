// Serves concurrent-map-bench: the Java driver, the same measurement as
// driver.c for java.util.concurrent.ConcurrentHashMap. It restates the
// workload of workload.h and prints the same rows. It does not pin threads;
// run.sh confines the process to the placement list.
//
//   java Bench.java --size N --dist uniform|zipf|one --threads 1,4 [...]

import java.io.DataInputStream;
import java.io.FileInputStream;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLongArray;

public final class Bench {
    static final long GOLDEN = 0x9E3779B97F4A7C15L;
    static final long MIX1 = 0xBF58476D1CE4E5B9L;
    static final long MIX2 = 0x94D049BB133111EBL;
    static final long M62 = (1L << 62) - 1;
    static final int ZIPF_LENGTH = 1 << 20;
    static final int BATCH = 256;
    static final int STRIDE = 8; // longs per published counter: one cache line

    static long mix64(long z) {
        z = (z ^ (z >>> 30)) * MIX1;
        z = (z ^ (z >>> 27)) * MIX2;
        return z ^ (z >>> 31);
    }

    static long key(long index) {
        long x = index & M62;
        x ^= x >>> 31;
        x = (x * MIX1) & M62;
        x ^= x >>> 29;
        x = (x * MIX2) & M62;
        x ^= x >>> 32;
        return x + 1;
    }

    static long seed(int mix, int threads, int thread) {
        return mix64(((long) mix << 48) ^ ((long) threads << 32) ^ thread);
    }

    // name, id, get, update, insert thresholds
    static final String[] MIX_NAMES = {"read", "mostly-read", "balanced", "update", "churn", "grow"};
    static final int[][] MIX_LIMITS = {
        {100, 100, 100}, {95, 100, 100}, {50, 100, 100}, {0, 100, 100}, {50, 50, 75}, {0, 0, 0}};
    static final int CHURN = 4, GROW = 5;

    static final class Map {
        final ConcurrentHashMap<Long, Long> table;

        Map(long capacity) {
            table = new ConcurrentHashMap<>((int) Math.min(capacity, 1 << 30));
        }

        // Returns the value plus one when present, else zero.
        long get(long k) {
            Long v = table.get(k);
            return v == null ? 0 : v + 1;
        }

        boolean insert(long k, long v) {
            return table.put(k, v) == null;
        }

        boolean remove(long k) {
            return table.remove(k) != null;
        }

        boolean update(long k) {
            return table.computeIfPresent(k, (key, v) -> v + 1) != null;
        }
    }

    static final class Totals {
        long getMisses, updateMisses, updates, inserted, removed, sink;
        long finished;
    }

    static volatile boolean go, stop;
    static Map map;
    static int[] limits;
    static int mixId, cellThreads, dist; // dist: 0 uniform, 1 zipf, 2 one
    static long range, fillCount;
    static int[][] zipf;
    static AtomicLongArray published;

    static void runMix(int t, Totals out) {
        Map m = map;
        int g = limits[0], u = limits[1], ins = limits[2];
        int[] z = zipf == null ? null : zipf[t];
        long state = seed(mixId, cellThreads, t);
        long pos = 0, ops = 0, gm = 0, um = 0, up = 0, in = 0, rem = 0, sink = 0;
        long r = range;
        int d = dist;
        while (true) {
            for (int j = 0; j < BATCH; j++) {
                state += GOLDEN;
                long x = mix64(state);
                int roll = (int) (((x & 0xffffffffL) * 100) >>> 32);
                long index = d == 0 ? ((x >>> 32) * r) >>> 32 : d == 1 ? z[(int) (pos++ & (ZIPF_LENGTH - 1))] : 0;
                long k = key(index);
                if (roll < g) {
                    long v = m.get(k);
                    if (v != 0) sink += v; else gm++;
                } else if (roll < u) {
                    if (m.update(k)) up++; else um++;
                } else if (roll < ins) {
                    if (m.insert(k, index)) in++;
                } else {
                    if (m.remove(k)) rem++;
                }
            }
            ops += BATCH;
            published.lazySet(t * STRIDE, ops);
            if (stop) break;
        }
        out.getMisses += gm;
        out.updateMisses += um;
        out.updates += up;
        out.inserted += in;
        out.removed += rem;
        out.sink += sink;
    }

    static Totals[] start(int threads, boolean fill, Thread[] pool) {
        cellThreads = threads;
        go = false;
        stop = false;
        published = new AtomicLongArray(threads * STRIDE);
        Totals[] totals = new Totals[threads];
        for (int t = 0; t < threads; t++) {
            final int id = t;
            totals[t] = new Totals();
            final Totals mine = totals[t];
            pool[t] = new Thread(() -> {
                while (!go) Thread.onSpinWait();
                if (fill) {
                    long n = 0;
                    for (long i = id; i < fillCount; i += cellThreads)
                        if (map.insert(key(i), i)) n++;
                    mine.inserted += n;
                    mine.finished = System.nanoTime();
                } else {
                    runMix(id, mine);
                }
            });
            pool[t].start();
        }
        return totals;
    }

    static long sumPublished(int threads) {
        long s = 0;
        for (int t = 0; t < threads; t++) s += published.get(t * STRIDE);
        return s;
    }

    static double[] runCell(int threads, int warmupMs, int durationMs, long[] acc) throws InterruptedException {
        Thread[] pool = new Thread[threads];
        Totals[] totals = start(threads, false, pool);
        go = true;
        Thread.sleep(warmupMs);
        long t0 = System.nanoTime();
        long a = sumPublished(threads);
        Thread.sleep(durationMs);
        long t1 = System.nanoTime();
        long b = sumPublished(threads);
        stop = true;
        for (Thread th : pool) th.join();
        for (Totals x : totals) {
            acc[0] += x.getMisses;
            acc[1] += x.updateMisses;
            acc[2] += x.updates;
            acc[3] += x.inserted;
            acc[4] += x.removed;
        }
        double seconds = (t1 - t0) * 1e-9;
        return new double[] {(b - a) / seconds / 1e6, b - a, seconds};
    }

    static double[] runFill(int threads, long count, long[] acc) throws InterruptedException {
        fillCount = count;
        Thread[] pool = new Thread[threads];
        Totals[] totals = start(threads, true, pool);
        long t0 = System.nanoTime();
        go = true;
        for (Thread th : pool) th.join();
        long last = t0;
        for (Totals x : totals) {
            last = Math.max(last, x.finished);
            acc[3] += x.inserted;
        }
        double seconds = (last - t0) * 1e-9;
        return new double[] {count / seconds / 1e6, count, seconds};
    }

    static String name = "java-chm";
    static String sizeText, distText, cpuText;
    static int[] cpus;

    static String cpus(int threads) {
        StringBuilder b = new StringBuilder();
        for (int t = 0; t < threads; t++) {
            if (t > 0) b.append('+');
            b.append(cpus[t % cpus.length]);
        }
        return b.toString();
    }

    static void row(String mix, int threads, double[] r, String check) {
        System.out.printf("%s,-,%s,%s,%s,%d,%s,%.3f,%d,%.4f,%s%n", name, sizeText, distText, mix, threads,
            threads > 0 ? cpus(threads) : "-", r == null ? 0.0 : r[0], r == null ? 0L : (long) r[1],
            r == null ? 0.0 : r[2], check);
        System.out.flush();
    }

    static int[] list(String text) {
        String[] parts = text.split(",");
        int[] out = new int[parts.length];
        for (int i = 0; i < parts.length; i++) out[i] = Integer.parseInt(parts[i]);
        return out;
    }

    static int[][] loadZipf(String path, long n, int threads) throws IOException {
        try (DataInputStream in = new DataInputStream(new FileInputStream(path))) {
            byte[] header = new byte[40];
            in.readFully(header);
            ByteBuffer h = ByteBuffer.wrap(header).order(ByteOrder.LITTLE_ENDIAN);
            if (!new String(header, 0, 7).equals("WFZIPF1")) throw new IOException("no Zipf header");
            long hn = h.getLong(8), streams = h.getLong(16), length = h.getLong(24);
            if (hn != n || length != ZIPF_LENGTH || streams < threads) throw new IOException("Zipf file mismatch");
            int[][] ranks = new int[threads][ZIPF_LENGTH];
            byte[] buffer = new byte[ZIPF_LENGTH * 4];
            for (int t = 0; t < threads; t++) {
                in.readFully(buffer);
                ByteBuffer.wrap(buffer).order(ByteOrder.LITTLE_ENDIAN).asIntBuffer().get(ranks[t]);
            }
            return ranks;
        }
    }

    public static void main(String[] args) throws Exception {
        long n = 0;
        int[] threadList = null;
        String mixText = "read,mostly-read,balanced,update,churn,grow", zipfPath = "";
        int warmupMs = 200, durationMs = 1000;
        distText = null;
        cpus = null;
        for (int i = 0; i + 1 < args.length; i += 2) {
            String k = args[i], v = args[i + 1];
            switch (k) {
                case "--size" -> n = Long.parseLong(v);
                case "--dist" -> distText = v;
                case "--threads" -> threadList = list(v);
                case "--mixes" -> mixText = v;
                case "--warmup-ms" -> warmupMs = Integer.parseInt(v);
                case "--duration-ms" -> durationMs = Integer.parseInt(v);
                case "--cpus" -> cpus = list(v);
                case "--zipf" -> zipfPath = v;
                default -> throw new IllegalArgumentException("unknown option " + k);
            }
        }
        dist = switch (distText) {
            case "uniform" -> 0;
            case "zipf" -> 1;
            case "one" -> 2;
            default -> throw new IllegalArgumentException("--dist");
        };
        if (cpus == null) {
            cpus = new int[Runtime.getRuntime().availableProcessors()];
            for (int i = 0; i < cpus.length; i++) cpus[i] = i;
        }
        sizeText = Long.toString(n);
        int maxThreads = 0;
        for (int t : threadList) maxThreads = Math.max(maxThreads, t);
        boolean[] wanted = new boolean[MIX_NAMES.length];
        for (String p : mixText.split(","))
            for (int m = 0; m < MIX_NAMES.length; m++) if (MIX_NAMES[m].equals(p)) wanted[m] = true;
        for (int m = 0; m < MIX_NAMES.length; m++) {
            if (dist == 2 && m != 3) wanted[m] = false;
            if (dist != 0 && (m == CHURN || m == GROW)) wanted[m] = false;
        }
        zipf = dist == 1 ? loadZipf(zipfPath, n, maxThreads) : null;
        long[] acc = new long[5];

        Runtime rt = Runtime.getRuntime();
        System.gc();
        long used0 = rt.totalMemory() - rt.freeMemory();
        map = new Map(n);
        double[] fill = runFill(maxThreads, n, acc);
        System.gc();
        long used1 = rt.totalMemory() - rt.freeMemory();
        row("prefill", maxThreads, fill, String.format("bytes-per-key=%.1f", (used1 - used0) / (double) n));
        long updates = 0;
        for (int m = 0; m < CHURN; m++) {
            if (!wanted[m]) continue;
            mixId = m;
            limits = MIX_LIMITS[m];
            range = n;
            for (int threads : threadList) {
                long[] cellAcc = new long[5];
                double[] r = runCell(threads, warmupMs, durationMs, cellAcc);
                updates += cellAcc[2];
                long misses = cellAcc[0] + cellAcc[1];
                row(MIX_NAMES[m], threads, r, misses == 0 ? "pass" : "fail:misses=" + misses);
            }
        }
        long sum = 0, missing = 0;
        for (long i = 0; i < n; i++) {
            long v = map.get(key(i));
            if (v == 0) missing++; else sum += v - 1;
        }
        long expected = n * (n - 1) / 2 + updates;
        row("check-values", 0, null,
            missing != 0 ? "fail:missing=" + missing : sum != expected ? "fail:sum-off-by=" + (sum - expected) : "pass");
        if (wanted[CHURN]) {
            long inserted = 0, removed = 0;
            mixId = CHURN;
            limits = MIX_LIMITS[CHURN];
            range = 2 * n;
            for (int threads : threadList) {
                long[] cellAcc = new long[5];
                double[] r = runCell(threads, warmupMs, durationMs, cellAcc);
                inserted += cellAcc[3];
                removed += cellAcc[4];
                row("churn", threads, r, "see-check-churn");
            }
            long live = 0;
            for (long i = 0; i < 2 * n; i++) if (map.get(key(i)) != 0) live++;
            long want = n + inserted - removed;
            row("check-churn", 0, null, live == want ? "pass" : "fail:live=" + live + ",expected=" + want);
        }
        map = null;
        if (wanted[GROW]) {
            for (int threads : threadList) {
                map = new Map(0);
                long[] cellAcc = new long[5];
                double[] r = runFill(threads, n, cellAcc);
                long found = 0;
                for (long i = 0; i < n; i++) if (map.get(key(i)) == i + 1) found++;
                row("grow", threads, r,
                    found == n && cellAcc[3] == n ? "pass" : "fail:found=" + found + ",inserted=" + cellAcc[3]);
                map = null;
            }
        }
    }
}
