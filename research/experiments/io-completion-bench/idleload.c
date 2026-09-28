/* Idle-connection memory of an echo server (research/investigations/io-model/
 * WAITS.md, Experiment 3).
 *
 *   idleload PORT CONNECTIONS SERVER_PID
 *
 * Reads the server's resident set and mapping count, opens CONNECTIONS
 * connections to 127.0.0.1:PORT and sends nothing on them, waits until the
 * server holds CONNECTIONS more descriptors than it did before, which is when
 * it has accepted every one, reads the two numbers again, and closes every
 * connection. It prints one tab-separated line of name=value fields. The
 * server is expected to exit once every connection it was started for has
 * closed; waiting for that is the caller's.
 *
 * There is no timeout: a server that never accepts every connection leaves
 * this tool waiting, and the caller sees that it did not finish. */
#define _GNU_SOURCE
#include <arpa/inet.h>
#include <dirent.h>
#include <errno.h>
#include <netinet/in.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

static long descriptors(long pid) {
    char path[64];
    long count = 0;
    DIR *directory;
    struct dirent *entry;
    snprintf(path, sizeof(path), "/proc/%ld/fd", pid);
    directory = opendir(path);
    if (directory == NULL) {
        return -1;
    }
    while ((entry = readdir(directory)) != NULL) {
        if (entry->d_name[0] != '.') {
            count += 1;
        }
    }
    closedir(directory);
    return count;
}

static long resident_kib(long pid) {
    char path[64];
    char line[256];
    long value = -1;
    FILE *status;
    snprintf(path, sizeof(path), "/proc/%ld/status", pid);
    status = fopen(path, "r");
    if (status == NULL) {
        return -1;
    }
    while (fgets(line, sizeof(line), status) != NULL) {
        if (strncmp(line, "VmRSS:", 6) == 0) {
            value = strtol(line + 6, NULL, 10);
            break;
        }
    }
    fclose(status);
    return value;
}

static long mappings(long pid) {
    char path[64];
    long count = 0;
    int character;
    FILE *maps;
    snprintf(path, sizeof(path), "/proc/%ld/maps", pid);
    maps = fopen(path, "r");
    if (maps == NULL) {
        return -1;
    }
    while ((character = fgetc(maps)) != EOF) {
        if (character == '\n') {
            count += 1;
        }
    }
    fclose(maps);
    return count;
}

static void pause_briefly(void) {
    struct timespec step = { 0, 10 * 1000 * 1000 };
    nanosleep(&step, NULL);
}

int main(int argc, char **argv) {
    long port, count, pid, index, base_descriptors, base_rss, base_maps;
    long loaded_rss, loaded_maps;
    int *sockets;
    struct sockaddr_in address;
    if (argc != 4) {
        fprintf(stderr, "usage: idleload PORT CONNECTIONS SERVER_PID\n");
        return 2;
    }
    port = strtol(argv[1], NULL, 10);
    count = strtol(argv[2], NULL, 10);
    pid = strtol(argv[3], NULL, 10);
    if (port <= 0 || port > 65535 || count <= 0 || pid <= 0) {
        fprintf(stderr, "idleload: a port, a positive count and a process id are required\n");
        return 2;
    }
    sockets = calloc((size_t)count, sizeof(*sockets));
    if (sockets == NULL) {
        fprintf(stderr, "idleload: no memory for %ld connections\n", count);
        return 1;
    }
    base_descriptors = descriptors(pid);
    base_rss = resident_kib(pid);
    base_maps = mappings(pid);
    if (base_descriptors < 0 || base_rss < 0 || base_maps < 0) {
        fprintf(stderr, "idleload: the server's /proc entries could not be read\n");
        return 1;
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons((unsigned short)port);
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    for (index = 0; index < count; index++) {
        for (;;) {
            int socket_descriptor = socket(AF_INET, SOCK_STREAM, 0);
            if (socket_descriptor < 0) {
                fprintf(stderr, "idleload: socket %ld: %s\n", index, strerror(errno));
                return 1;
            }
            if (connect(socket_descriptor, (struct sockaddr *)&address, sizeof(address)) == 0) {
                sockets[index] = socket_descriptor;
                break;
            }
            /* A full accept queue refuses the handshake; the server drains it. */
            close(socket_descriptor);
            if (errno != ECONNREFUSED && errno != EAGAIN && errno != ETIMEDOUT) {
                fprintf(stderr, "idleload: connect %ld: %s\n", index, strerror(errno));
                return 1;
            }
            pause_briefly();
        }
    }
    for (;;) {
        long held = descriptors(pid);
        if (held < 0) {
            fprintf(stderr, "idleload: the server exited before it accepted every connection\n");
            return 1;
        }
        if (held - base_descriptors >= count) {
            break;
        }
        pause_briefly();
    }
    loaded_rss = resident_kib(pid);
    loaded_maps = mappings(pid);
    printf("connections=%ld\tbase_rss_kib=%ld\tbase_maps=%ld\tloaded_rss_kib=%ld\t"
           "loaded_maps=%ld\trss_per_connection_kib=%.2f\tmaps_per_connection=%.3f\n",
           count, base_rss, base_maps, loaded_rss, loaded_maps,
           (double)(loaded_rss - base_rss) / (double)count,
           (double)(loaded_maps - base_maps) / (double)count);
    for (index = 0; index < count; index++) {
        close(sockets[index]);
    }
    free(sockets);
    return 0;
}
