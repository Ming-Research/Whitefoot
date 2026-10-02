// Serves concurrent-map-bench: the Go driver, the same measurement as
// driver.c for Go's sync.Map (`go-syncmap`, whose update is a
// compare-and-swap loop and so flagged optimistic) and xsync's Map
// (`go-xsync`, whose Compute runs once under its bucket's lock). It restates
// the workload of workload.h and prints the same rows. It does not pin
// goroutines; run.sh confines the process to the placement list.
//
//	go run . --impl syncmap|xsync --size N --dist uniform|zipf|one --threads 1,4 [...]
package main

import (
	"encoding/binary"
	"flag"
	"fmt"
	"io"
	"os"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/puzpuzpuz/xsync/v4"
)

const (
	golden     = 0x9E3779B97F4A7C15
	mix1       = 0xBF58476D1CE4E5B9
	mix2       = 0x94D049BB133111EB
	m62        = (1 << 62) - 1
	zipfLength = 1 << 20
	batch      = 256
)

func mix64(z uint64) uint64 {
	z = (z ^ (z >> 30)) * mix1
	z = (z ^ (z >> 27)) * mix2
	return z ^ (z >> 31)
}

func key(index uint64) uint64 {
	x := index & m62
	x ^= x >> 31
	x = (x * mix1) & m62
	x ^= x >> 29
	x = (x * mix2) & m62
	x ^= x >> 32
	return x + 1
}

func seed(mix, threads, thread int) uint64 {
	return mix64(uint64(mix)<<48 ^ uint64(threads)<<32 ^ uint64(thread))
}

type cmap interface {
	get(k uint64) (uint64, bool)
	insert(k, v uint64) bool
	remove(k uint64) bool
	update(k uint64) bool
}

type syncMap struct{ m sync.Map }

func (s *syncMap) get(k uint64) (uint64, bool) {
	v, ok := s.m.Load(k)
	if !ok {
		return 0, false
	}
	return v.(uint64), true
}
func (s *syncMap) insert(k, v uint64) bool {
	_, loaded := s.m.Swap(k, v)
	return !loaded
}
func (s *syncMap) remove(k uint64) bool {
	_, loaded := s.m.LoadAndDelete(k)
	return loaded
}
func (s *syncMap) update(k uint64) bool {
	for {
		v, ok := s.m.Load(k)
		if !ok {
			return false
		}
		if s.m.CompareAndSwap(k, v, v.(uint64)+1) {
			return true
		}
	}
}

type xsyncMap struct{ m *xsync.Map[uint64, uint64] }

func (s *xsyncMap) get(k uint64) (uint64, bool) { return s.m.Load(k) }
func (s *xsyncMap) insert(k, v uint64) bool {
	_, loaded := s.m.LoadAndStore(k, v)
	return !loaded
}
func (s *xsyncMap) remove(k uint64) bool {
	_, loaded := s.m.LoadAndDelete(k)
	return loaded
}
func (s *xsyncMap) update(k uint64) bool {
	_, ok := s.m.Compute(k, func(old uint64, loaded bool) (uint64, xsync.ComputeOp) {
		if !loaded {
			return 0, xsync.CancelOp
		}
		return old + 1, xsync.UpdateOp
	})
	return ok
}

var mixNames = []string{"read", "mostly-read", "balanced", "update", "churn", "grow"}
var mixLimits = [][3]uint32{{100, 100, 100}, {95, 100, 100}, {50, 100, 100}, {0, 100, 100}, {50, 50, 75}, {0, 0, 0}}

const churn, grow = 4, 5

type totals struct {
	getMisses, updateMisses, updates, inserted, removed, sink uint64
	finished                                                  time.Time
	_                                                         [64]byte
}

type counter struct {
	n atomic.Uint64
	_ [56]byte
}

type cell struct {
	m       cmap
	mix     int
	threads int
	dist    int // 0 uniform, 1 zipf, 2 one
	rng     uint64
	zipf    [][]uint32
	stop    atomic.Bool
	pub     []counter
}

func (c *cell) run(t int, out *totals) {
	m := c.m
	lim := mixLimits[c.mix]
	var z []uint32
	if c.zipf != nil {
		z = c.zipf[t]
	}
	state := seed(c.mix, c.threads, t)
	var pos, ops, gm, um, up, ins, rem, sink uint64
	for {
		for j := 0; j < batch; j++ {
			state += golden
			x := mix64(state)
			roll := uint32(((x & 0xffffffff) * 100) >> 32)
			var index uint64
			switch c.dist {
			case 0:
				index = ((x >> 32) * c.rng) >> 32
			case 1:
				index = uint64(z[pos&(zipfLength-1)])
				pos++
			}
			k := key(index)
			switch {
			case roll < lim[0]:
				if v, ok := m.get(k); ok {
					sink += v
				} else {
					gm++
				}
			case roll < lim[1]:
				if m.update(k) {
					up++
				} else {
					um++
				}
			case roll < lim[2]:
				if m.insert(k, index) {
					ins++
				}
			default:
				if m.remove(k) {
					rem++
				}
			}
		}
		ops += batch
		c.pub[t].n.Store(ops)
		if c.stop.Load() {
			break
		}
	}
	out.getMisses += gm
	out.updateMisses += um
	out.updates += up
	out.inserted += ins
	out.removed += rem
	out.sink += sink
}

func sum(c *cell) uint64 {
	var s uint64
	for i := range c.pub {
		s += c.pub[i].n.Load()
	}
	return s
}

func runCell(c *cell, threads int, warmup, duration time.Duration) (float64, uint64, float64, totals) {
	c.threads = threads
	c.stop.Store(false)
	c.pub = make([]counter, threads)
	outs := make([]totals, threads)
	var wg sync.WaitGroup
	for t := 0; t < threads; t++ {
		wg.Add(1)
		go func(t int) {
			defer wg.Done()
			runtime.LockOSThread()
			c.run(t, &outs[t])
		}(t)
	}
	time.Sleep(warmup)
	t0 := time.Now()
	a := sum(c)
	time.Sleep(duration)
	t1 := time.Now()
	b := sum(c)
	c.stop.Store(true)
	wg.Wait()
	var all totals
	for _, o := range outs {
		all.getMisses += o.getMisses
		all.updateMisses += o.updateMisses
		all.updates += o.updates
		all.inserted += o.inserted
		all.removed += o.removed
	}
	sec := t1.Sub(t0).Seconds()
	return float64(b-a) / sec / 1e6, b - a, sec, all
}

func runFill(m cmap, threads int, count uint64) (float64, uint64, float64, uint64) {
	var wg sync.WaitGroup
	var inserted atomic.Uint64
	start := make(chan struct{})
	finished := make([]time.Time, threads)
	for t := 0; t < threads; t++ {
		wg.Add(1)
		go func(t int) {
			defer wg.Done()
			runtime.LockOSThread()
			<-start
			var n uint64
			for i := uint64(t); i < count; i += uint64(threads) {
				if m.insert(key(i), i) {
					n++
				}
			}
			inserted.Add(n)
			finished[t] = time.Now()
		}(t)
	}
	t0 := time.Now()
	close(start)
	wg.Wait()
	last := t0
	for _, f := range finished {
		if f.After(last) {
			last = f
		}
	}
	sec := last.Sub(t0).Seconds()
	return float64(count) / sec / 1e6, count, sec, inserted.Load()
}

func list(text string) []int {
	var out []int
	for _, p := range strings.Split(text, ",") {
		n, err := strconv.Atoi(p)
		if err != nil {
			panic(err)
		}
		out = append(out, n)
	}
	return out
}

func loadZipf(path string, n uint64, threads int) [][]uint32 {
	f, err := os.Open(path)
	if err != nil {
		panic(err)
	}
	defer f.Close()
	header := make([]byte, 40)
	if _, err := io.ReadFull(f, header); err != nil || string(header[:7]) != "WFZIPF1" {
		panic("no Zipf header")
	}
	if binary.LittleEndian.Uint64(header[8:]) != n || binary.LittleEndian.Uint64(header[24:]) != zipfLength ||
		binary.LittleEndian.Uint64(header[16:]) < uint64(threads) {
		panic("Zipf file mismatch")
	}
	ranks := make([][]uint32, threads)
	buf := make([]byte, zipfLength*4)
	for t := range ranks {
		if _, err := io.ReadFull(f, buf); err != nil {
			panic(err)
		}
		ranks[t] = make([]uint32, zipfLength)
		for i := range ranks[t] {
			ranks[t][i] = binary.LittleEndian.Uint32(buf[i*4:])
		}
	}
	return ranks
}

func main() {
	impl := flag.String("impl", "xsync", "syncmap or xsync")
	size := flag.Uint64("size", 0, "keys")
	distText := flag.String("dist", "", "uniform, zipf or one")
	threadText := flag.String("threads", "", "thread counts")
	mixText := flag.String("mixes", "read,mostly-read,balanced,update,churn,grow", "mixes")
	warmupMs := flag.Int("warmup-ms", 200, "warmup")
	durationMs := flag.Int("duration-ms", 1000, "duration")
	cpuText := flag.String("cpus", "", "placement list")
	zipfPath := flag.String("zipf", "", "Zipf rank file")
	flag.Parse()
	n := *size
	threads := list(*threadText)
	cpus := []int{}
	if *cpuText != "" {
		cpus = list(*cpuText)
	} else {
		for i := 0; i < runtime.NumCPU(); i++ {
			cpus = append(cpus, i)
		}
	}
	dist := map[string]int{"uniform": 0, "zipf": 1, "one": 2}[*distText]
	name, flags := "go-xsync", "-"
	newMap := func(capacity uint64) cmap {
		return &xsyncMap{xsync.NewMap[uint64, uint64](xsync.WithPresize(int(capacity)))}
	}
	if *impl == "syncmap" {
		name, flags = "go-syncmap", "optimistic-update"
		newMap = func(uint64) cmap { return &syncMap{} }
	}
	maxThreads := 0
	for _, t := range threads {
		maxThreads = max(maxThreads, t)
	}
	wanted := make([]bool, len(mixNames))
	for _, p := range strings.Split(*mixText, ",") {
		for m, nm := range mixNames {
			if nm == p {
				wanted[m] = true
			}
		}
	}
	for m := range wanted {
		if (dist == 2 && m != 3) || (dist != 0 && (m == churn || m == grow)) {
			wanted[m] = false
		}
	}
	cpuList := func(t int) string {
		parts := make([]string, t)
		for i := range parts {
			parts[i] = strconv.Itoa(cpus[i%len(cpus)])
		}
		return strings.Join(parts, "+")
	}
	row := func(mix string, t int, mops float64, ops uint64, sec float64, check string) {
		cl := "-"
		if t > 0 {
			cl = cpuList(t)
		}
		fmt.Printf("%s,%s,%d,%s,%s,%d,%s,%.3f,%d,%.4f,%s\n", name, flags, n, *distText, mix, t, cl, mops, ops, sec, check)
	}
	c := &cell{dist: dist}
	if dist == 1 {
		c.zipf = loadZipf(*zipfPath, n, maxThreads)
	}
	var ms runtime.MemStats
	runtime.GC()
	runtime.ReadMemStats(&ms)
	heap0 := ms.HeapAlloc
	c.m = newMap(n)
	mops, ops, sec, _ := runFill(c.m, maxThreads, n)
	runtime.GC()
	runtime.ReadMemStats(&ms)
	row("prefill", maxThreads, mops, ops, sec, fmt.Sprintf("bytes-per-key=%.1f", float64(ms.HeapAlloc-heap0)/float64(n)))
	warm, dur := time.Duration(*warmupMs)*time.Millisecond, time.Duration(*durationMs)*time.Millisecond
	var updates uint64
	for m := 0; m < churn; m++ {
		if !wanted[m] {
			continue
		}
		c.mix, c.rng = m, n
		for _, t := range threads {
			mops, ops, sec, all := runCell(c, t, warm, dur)
			updates += all.updates
			check := "pass"
			if all.getMisses+all.updateMisses != 0 {
				check = fmt.Sprintf("fail:misses=%d", all.getMisses+all.updateMisses)
			}
			row(mixNames[m], t, mops, ops, sec, check)
		}
	}
	var total, missing uint64
	for i := uint64(0); i < n; i++ {
		if v, ok := c.m.get(key(i)); ok {
			total += v
		} else {
			missing++
		}
	}
	expected := n*(n-1)/2 + updates
	check := "pass"
	if missing != 0 {
		check = fmt.Sprintf("fail:missing=%d", missing)
	} else if total != expected {
		check = fmt.Sprintf("fail:sum-off-by=%d", int64(total-expected))
	}
	row("check-values", 0, 0, 0, 0, check)
	if wanted[churn] {
		var inserted, removed uint64
		c.mix, c.rng = churn, 2*n
		for _, t := range threads {
			mops, ops, sec, all := runCell(c, t, warm, dur)
			inserted += all.inserted
			removed += all.removed
			row("churn", t, mops, ops, sec, "see-check-churn")
		}
		var live uint64
		for i := uint64(0); i < 2*n; i++ {
			if _, ok := c.m.get(key(i)); ok {
				live++
			}
		}
		want := n + inserted - removed
		check := "pass"
		if live != want {
			check = fmt.Sprintf("fail:live=%d,expected=%d", live, want)
		}
		row("check-churn", 0, 0, 0, 0, check)
	}
	c.m = nil
	if wanted[grow] {
		for _, t := range threads {
			m := newMap(0)
			mops, ops, sec, inserted := runFill(m, t, n)
			var found uint64
			for i := uint64(0); i < n; i++ {
				if v, ok := m.get(key(i)); ok && v == i {
					found++
				}
			}
			check := "pass"
			if found != n || inserted != n {
				check = fmt.Sprintf("fail:found=%d,inserted=%d", found, inserted)
			}
			row("grow", t, mops, ops, sec, check)
		}
	}
}
