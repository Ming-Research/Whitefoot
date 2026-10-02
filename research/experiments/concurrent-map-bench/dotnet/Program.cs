// Serves concurrent-map-bench: the .NET driver, the same measurement as
// driver.c for System.Collections.Concurrent.ConcurrentDictionary
// (`dotnet-cd`). Its reads take no lock and its writes take one of its
// striped locks; it has no update that runs once under a lock, so update is a
// compare-and-swap loop over TryUpdate and flagged optimistic. It restates
// the workload of workload.h and prints the same rows. It does not pin
// threads; run.sh confines the process to the placement list.
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Threading;

static class Bench
{
    const ulong Golden = 0x9E3779B97F4A7C15UL, Mix1 = 0xBF58476D1CE4E5B9UL, Mix2 = 0x94D049BB133111EBUL;
    const ulong M62 = (1UL << 62) - 1;
    const int ZipfLength = 1 << 20, Batch = 256, Stride = 8;

    static ulong Mix64(ulong z)
    {
        unchecked
        {
            z = (z ^ (z >> 30)) * Mix1;
            z = (z ^ (z >> 27)) * Mix2;
            return z ^ (z >> 31);
        }
    }

    static ulong Key(ulong index)
    {
        unchecked
        {
            ulong x = index & M62;
            x ^= x >> 31;
            x = (x * Mix1) & M62;
            x ^= x >> 29;
            x = (x * Mix2) & M62;
            x ^= x >> 32;
            return x + 1;
        }
    }

    static ulong Seed(int mix, int threads, int thread) =>
        Mix64(((ulong)mix << 48) ^ ((ulong)threads << 32) ^ (ulong)thread);

    static readonly string[] MixNames = { "read", "mostly-read", "balanced", "update", "churn", "grow" };
    static readonly uint[][] MixLimits =
        { new uint[] { 100, 100, 100 }, new uint[] { 95, 100, 100 }, new uint[] { 50, 100, 100 },
          new uint[] { 0, 100, 100 }, new uint[] { 50, 50, 75 }, new uint[] { 0, 0, 0 } };
    const int Churn = 4, Grow = 5;

    static ConcurrentDictionary<ulong, ulong> map;

    static bool Insert(ulong k, ulong v)
    {
        while (true)
        {
            if (map.TryAdd(k, v)) return true;
            if (map.TryGetValue(k, out ulong old) && map.TryUpdate(k, v, old)) return false;
        }
    }

    static bool Update(ulong k)
    {
        while (true)
        {
            if (!map.TryGetValue(k, out ulong v)) return false;
            if (map.TryUpdate(k, v + 1, v)) return true;
        }
    }

    sealed class Totals
    {
        public ulong GetMisses, UpdateMisses, Updates, Inserted, Removed, Sink;
        public long Finished;
    }

    static volatile bool go, stop;
    static int mixId, cellThreads, dist;
    static ulong range, fillCount;
    static uint[][] zipf;
    static long[] published;

    static void RunMix(int t, Totals o)
    {
        uint[] lim = MixLimits[mixId];
        uint[] z = zipf?[t];
        ulong state = Seed(mixId, cellThreads, t);
        ulong pos = 0, gm = 0, um = 0, up = 0, ins = 0, rem = 0, sink = 0;
        long ops = 0;
        ulong r = range;
        while (true)
        {
            for (int j = 0; j < Batch; j++)
            {
                unchecked
                {
                    state += Golden;
                    ulong x = Mix64(state);
                    uint roll = (uint)(((x & 0xffffffffUL) * 100) >> 32);
                    ulong index = dist == 0 ? ((x >> 32) * r) >> 32 : dist == 1 ? z[(int)(pos++ & (ZipfLength - 1))] : 0;
                    ulong k = Key(index);
                    if (roll < lim[0])
                    {
                        if (map.TryGetValue(k, out ulong v)) sink += v; else gm++;
                    }
                    else if (roll < lim[1])
                    {
                        if (Update(k)) up++; else um++;
                    }
                    else if (roll < lim[2])
                    {
                        if (Insert(k, index)) ins++;
                    }
                    else
                    {
                        if (map.TryRemove(k, out _)) rem++;
                    }
                }
            }
            ops += Batch;
            Volatile.Write(ref published[t * Stride], ops);
            if (stop) break;
        }
        o.GetMisses += gm; o.UpdateMisses += um; o.Updates += up; o.Inserted += ins; o.Removed += rem; o.Sink += sink;
    }

    static (Thread[], Totals[]) Start(int threads, bool fill)
    {
        cellThreads = threads;
        go = false;
        stop = false;
        published = new long[threads * Stride];
        var pool = new Thread[threads];
        var totals = new Totals[threads];
        for (int t = 0; t < threads; t++)
        {
            int id = t;
            var mine = totals[t] = new Totals();
            pool[t] = new Thread(() =>
            {
                while (!go) Thread.SpinWait(1);
                if (fill)
                {
                    ulong n = 0;
                    for (ulong i = (ulong)id; i < fillCount; i += (ulong)cellThreads)
                        if (Insert(Key(i), i)) n++;
                    mine.Inserted += n;
                    mine.Finished = Stopwatch.GetTimestamp();
                }
                else RunMix(id, mine);
            });
            pool[t].Start();
        }
        return (pool, totals);
    }

    static long Published(int threads)
    {
        long s = 0;
        for (int t = 0; t < threads; t++) s += Volatile.Read(ref published[t * Stride]);
        return s;
    }

    static (double, long, double, Totals) RunCell(int threads, int warmupMs, int durationMs)
    {
        var (pool, totals) = Start(threads, false);
        go = true;
        Thread.Sleep(warmupMs);
        long t0 = Stopwatch.GetTimestamp();
        long a = Published(threads);
        Thread.Sleep(durationMs);
        long t1 = Stopwatch.GetTimestamp();
        long b = Published(threads);
        stop = true;
        foreach (var th in pool) th.Join();
        var all = new Totals();
        foreach (var x in totals)
        {
            all.GetMisses += x.GetMisses; all.UpdateMisses += x.UpdateMisses; all.Updates += x.Updates;
            all.Inserted += x.Inserted; all.Removed += x.Removed;
        }
        double sec = (t1 - t0) / (double)Stopwatch.Frequency;
        return ((b - a) / sec / 1e6, b - a, sec, all);
    }

    static (double, long, double, ulong) RunFill(int threads, ulong count)
    {
        fillCount = count;
        var (pool, totals) = Start(threads, true);
        long t0 = Stopwatch.GetTimestamp();
        go = true;
        foreach (var th in pool) th.Join();
        long last = t0;
        ulong inserted = 0;
        foreach (var x in totals) { last = Math.Max(last, x.Finished); inserted += x.Inserted; }
        double sec = (last - t0) / (double)Stopwatch.Frequency;
        return (count / sec / 1e6, (long)count, sec, inserted);
    }

    static uint[][] LoadZipf(string path, ulong n, int threads)
    {
        using var f = File.OpenRead(path);
        using var br = new BinaryReader(f);
        byte[] magic = br.ReadBytes(8);
        if (System.Text.Encoding.ASCII.GetString(magic, 0, 7) != "WFZIPF1") throw new IOException("no Zipf header");
        ulong hn = br.ReadUInt64(), streams = br.ReadUInt64(), length = br.ReadUInt64();
        br.ReadDouble();
        if (hn != n || length != ZipfLength || streams < (ulong)threads) throw new IOException("Zipf file mismatch");
        var ranks = new uint[threads][];
        for (int t = 0; t < threads; t++)
        {
            byte[] bytes = br.ReadBytes(ZipfLength * 4);
            ranks[t] = new uint[ZipfLength];
            Buffer.BlockCopy(bytes, 0, ranks[t], 0, bytes.Length);
        }
        return ranks;
    }

    static int Main(string[] args)
    {
        ulong n = 0;
        int[] threads = null, cpus = null;
        string distText = null, mixText = "read,mostly-read,balanced,update,churn,grow", zipfPath = "";
        int warmupMs = 200, durationMs = 1000;
        for (int i = 0; i + 1 < args.Length; i += 2)
        {
            string k = args[i], v = args[i + 1];
            switch (k)
            {
                case "--size": n = ulong.Parse(v); break;
                case "--dist": distText = v; break;
                case "--threads": threads = v.Split(',').Select(int.Parse).ToArray(); break;
                case "--mixes": mixText = v; break;
                case "--warmup-ms": warmupMs = int.Parse(v); break;
                case "--duration-ms": durationMs = int.Parse(v); break;
                case "--cpus": cpus = v.Split(',').Select(int.Parse).ToArray(); break;
                case "--zipf": zipfPath = v; break;
                default: Console.Error.WriteLine($"unknown option {k}"); return 2;
            }
        }
        cpus ??= Enumerable.Range(0, Environment.ProcessorCount).ToArray();
        dist = distText switch { "uniform" => 0, "zipf" => 1, "one" => 2, _ => -1 };
        if (dist < 0 || n == 0 || threads == null) { Console.Error.WriteLine("--size, --dist and --threads are required"); return 2; }
        int maxThreads = threads.Max();
        var wanted = new bool[MixNames.Length];
        foreach (var p in mixText.Split(',')) { int m = Array.IndexOf(MixNames, p); if (m >= 0) wanted[m] = true; }
        for (int m = 0; m < wanted.Length; m++)
            if ((dist == 2 && m != 3) || (dist != 0 && (m == Churn || m == Grow))) wanted[m] = false;
        zipf = dist == 1 ? LoadZipf(zipfPath, n, maxThreads) : null;
        string Cpus(int t) => string.Join("+", Enumerable.Range(0, t).Select(i => cpus[i % cpus.Length]));
        void Row(string mix, int t, double mops, long ops, double sec, string check) =>
            Console.WriteLine($"dotnet-cd,optimistic-update,{n},{distText},{mix},{t},{(t > 0 ? Cpus(t) : "-")},{mops:F3},{ops},{sec:F4},{check}");

        GC.Collect();
        long heap0 = GC.GetTotalMemory(true);
        map = new ConcurrentDictionary<ulong, ulong>(Environment.ProcessorCount, (int)Math.Min(n, int.MaxValue));
        var (fm, fo, fs, _) = RunFill(maxThreads, n);
        long heap1 = GC.GetTotalMemory(true);
        Row("prefill", maxThreads, fm, fo, fs, $"bytes-per-key={(heap1 - heap0) / (double)n:F1}");
        ulong updates = 0;
        for (int m = 0; m < Churn; m++)
        {
            if (!wanted[m]) continue;
            mixId = m;
            range = n;
            foreach (int t in threads)
            {
                var (mops, ops, sec, all) = RunCell(t, warmupMs, durationMs);
                updates += all.Updates;
                ulong misses = all.GetMisses + all.UpdateMisses;
                Row(MixNames[m], t, mops, ops, sec, misses == 0 ? "pass" : $"fail:misses={misses}");
            }
        }
        ulong sum = 0, missing = 0;
        for (ulong i = 0; i < n; i++)
            if (map.TryGetValue(Key(i), out ulong v)) sum += v; else missing++;
        ulong expected = unchecked(n * (n - 1) / 2 + updates);
        Row("check-values", 0, 0, 0, 0,
            missing != 0 ? $"fail:missing={missing}" : sum != expected ? $"fail:sum-off-by={unchecked((long)(sum - expected))}" : "pass");
        if (wanted[Churn])
        {
            ulong inserted = 0, removed = 0;
            mixId = Churn;
            range = 2 * n;
            foreach (int t in threads)
            {
                var (mops, ops, sec, all) = RunCell(t, warmupMs, durationMs);
                inserted += all.Inserted;
                removed += all.Removed;
                Row("churn", t, mops, ops, sec, "see-check-churn");
            }
            ulong live = 0;
            for (ulong i = 0; i < 2 * n; i++) if (map.ContainsKey(Key(i))) live++;
            ulong want = n + inserted - removed;
            Row("check-churn", 0, 0, 0, 0, live == want ? "pass" : $"fail:live={live},expected={want}");
        }
        map = null;
        if (wanted[Grow])
        {
            foreach (int t in threads)
            {
                map = new ConcurrentDictionary<ulong, ulong>(Environment.ProcessorCount, 31);
                var (mops, ops, sec, inserted) = RunFill(t, n);
                ulong found = 0;
                for (ulong i = 0; i < n; i++) if (map.TryGetValue(Key(i), out ulong v) && v == i) found++;
                Row("grow", t, mops, ops, sec, found == n && inserted == n ? "pass" : $"fail:found={found},inserted={inserted}");
                map = null;
            }
        }
        return 0;
    }
}
