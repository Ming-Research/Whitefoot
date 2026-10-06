use super::support::{build_program, compile_graph};
use std::path::Path;

fn request(parts: &[&[u8]], bytes: &mut Vec<u8>) {
    bytes.extend_from_slice(format!("*{}\r\n", parts.len()).as_bytes());
    for part in parts {
        bytes.extend_from_slice(format!("${}\r\n", part.len()).as_bytes());
        bytes.extend_from_slice(part);
        bytes.extend_from_slice(b"\r\n");
    }
}

#[test]
fn firn_scripting_dispatch_cache_and_errors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/programs/firn-scripting");
    let program = build_program(&compile_graph(&root, "test"));
    let mut input = Vec::new();
    // Each new dispatch name must reach its implementation, including EVALSHA
    // when it cannot find a script; unknown-command would fail exact bytes.
    let sha = b"1fa00e76656cc152ad327c13fe365858fd7be306"; // SHA1("return 42")
    for command in [
        vec![b"EVAL".as_slice(), b"return 42", b"0"],
        vec![b"SCRIPT", b"EXISTS", sha],
        vec![b"EVALSHA", sha, b"0"],
        vec![b"SCRIPT", b"LOAD", b"return 42"],
        vec![b"SCRIPT", b"FLUSH", b"ASYNC"],
        vec![b"SCRIPT", b"EXISTS", sha],
        vec![b"EVALSHA", sha, b"0"],
        vec![
            b"EVAL",
            b"return {KEYS[1],ARGV[1],false,true}",
            b"1",
            b"key",
            b"arg",
        ],
        vec![b"EVAL", b"return 0", b"-1"],
        vec![b"EVAL", b"return 0", b"1"],
        vec![b"EVAL", b"return 0", b"+0"],
        vec![b"SCRIPT", b"EXISTS"],
        vec![b"SCRIPT", b"FLUSH", b"wrong"],
        vec![b"EVAL", b"return redis.pcall('GET','key')", b"0"],
        vec![b"EVAL", b"return redis.pcall('SUBSCRIBE','channel')", b"0"],
        vec![b"EVAL", b"redis.setresp(3);return {false,true}", b"0"],
        vec![
            b"EVAL",
            b"local n=0;for i=1,3000 do n=n+1 end;return n",
            b"0",
        ],
        vec![
            b"EVAL",
            b"local m=getmetatable(_G);return pcall(function()m.__index=nil end)",
            b"0",
        ],
        vec![b"SCRIPT", b"LOAD"],
        vec![b"EVALSHA", b"short", b"wrong"],
        vec![b"SCRIPT", b"FLUSH", b"SYNC"],
        vec![b"EVAL", b"return 42", b"0"],
    ] {
        request(&command, &mut input);
    }
    let mut expected = b":42\r\n*1\r\n:1\r\n:42\r\n$40\r\n1fa00e76656cc152ad327c13fe365858fd7be306\r\n+OK\r\n*1\r\n:0\r\n-NOSCRIPT No matching script. Please use EVAL.\r\n*4\r\n$3\r\nkey\r\n$3\r\narg\r\n$-1\r\n:1\r\n-ERR Number of keys can't be negative\r\n-ERR Number of keys can't be greater than number of args\r\n-ERR value is not an integer or out of range\r\n*0\r\n-ERR SCRIPT FLUSH only support SYNC|ASYNC option\r\n-ERR This Redis command is not yet supported in firn scripts\r\n-ERR This Redis command is not allowed from script\r\n*2\r\n:0\r\n:1\r\n:3000\r\n$-1\r\n-ERR wrong number of arguments for 'script|load' command\r\n-NOSCRIPT No matching script. Please use EVAL.\r\n+OK\r\n:42\r\n".to_vec();
    // Redis 7.0.15 eval.c retains every compiled source until scriptingReset;
    // exercise several registry growth boundaries and retain the first EVAL.
    for n in 0..64 {
        let body = format!("return {n} -- retention probe");
        request(&[b"EVAL", body.as_bytes(), b"0"], &mut input);
        expected.extend_from_slice(format!(":{n}\r\n").as_bytes());
    }
    for command in [
        vec![b"SCRIPT".as_slice(), b"EXISTS", sha],
        vec![b"EVALSHA", b"1FA00E76656CC152AD327C13FE365858FD7BE306", b"0"],
        vec![b"SCRIPT", b"EXISTS", b"1FA00E76656CC152AD327C13FE365858FD7BE306"],
        vec![b"EVAL"],
        vec![b"EVALSHA"],
        vec![b"SCRIPT"],
        vec![b"SCRIPT", b"FLUSH", b"SYNC", b"extra"],
        vec![b"EVAL", b"return {redis.pcall('GET').err,redis.pcall('BLPOP','k',0).err,redis.pcall('ACL','CAT').err,redis.pcall('not_a_command').err}", b"0"],
        vec![b"EVAL", b"cjson.encode_keep_buffer(false);return cjson.encode_keep_buffer()", b"0"],
        vec![b"SCRIPT", b"FLUSH", b"SYNC"],
        vec![b"SCRIPT", b"EXISTS", sha],
        vec![b"EVALSHA", sha, b"0"],
        vec![b"EVAL", b"return cjson.encode_keep_buffer()", b"0"],
        vec![b"SCRIPT", b"FLUSH", b"ASYNC"],
        vec![b"EVALSHA", sha, b"0"],
    ] { request(&command, &mut input); }
    expected.extend_from_slice(b"*1\r\n:1\r\n:42\r\n*1\r\n:0\r\n-ERR wrong number of arguments for 'eval' command\r\n-ERR wrong number of arguments for 'evalsha' command\r\n-ERR wrong number of arguments for 'script' command\r\n-ERR SCRIPT FLUSH only support SYNC|ASYNC option\r\n*4\r\n$58\r\nERR Wrong number of args calling Redis command from script\r\n$49\r\nERR This Redis command is not allowed from script\r\n$49\r\nERR This Redis command is not allowed from script\r\n$44\r\nERR Unknown Redis command called from script\r\n$-1\r\n+OK\r\n*1\r\n:0\r\n-NOSCRIPT No matching script. Please use EVAL.\r\n:1\r\n+OK\r\n-NOSCRIPT No matching script. Please use EVAL.\r\n");
    request(&[b"EVAL", b"return redis.pcall().err", b"0"], &mut input);
    expected.extend_from_slice(
        b"$64\r\nERR Please specify at least one argument for this redis lib call\r\n",
    );
    request(
        &[
            b"EVAL",
            b"return redis.error_reply('ERR \\r\\nprobe\\r\\n').err",
            b"0",
        ],
        &mut input,
    );
    expected.extend_from_slice(b"$9\r\nERR probe\r\n");
    request(
        &[
            b"EVAL",
            b"return redis.pcall('BLPOP','k',0).ignore_error_stats_update",
            b"0",
        ],
        &mut input,
    );
    expected.extend_from_slice(b"$-1\r\n");
    request(&[b"SCRIPT", b"unknown"], &mut input);
    expected.extend_from_slice(
        b"-ERR unknown subcommand or wrong number of arguments for 'unknown'. Try SCRIPT HELP.\r\n",
    );
    for native in [false, true] {
        let output = program.run_with_piped_input(&input, native);
        assert!(output.status.success(), "driver failed: {output:?}");
        assert_eq!(output.stdout, expected, "native={native}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}
