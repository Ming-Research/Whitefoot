/* Idle-connection memory of an echo server (research/investigations/io-model/
 * WAITS.md, Experiment 3).
 *
 *   idleload PORT CONNECTIONS SERVER_PID
 *
 * Reads the server's resident set and mapping count, opens CONNECTIONS
 * connections to 127.0.0.1:PORT and sends nothing on them, waits until the
 * server holds CONNECTIONS descriptors established on PORT, which is when it
 * has accepted every one, reads the two numbers again, and closes every
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

static int compare_inodes(const void *left, const void *right) {
    unsigned long a = *(const unsigned long *)left, b = *(const unsigned long *)right;
    return (a > b) - (a < b);
}

/* The inodes of every connection the kernel's TCP table lists as established
 * on local port `port`, sorted: the peer ends of the connections to the
 * server and the server's accepted ends both appear, and only the latter are
 * the server's descriptors. Returns the count, or -1. */
static long established_inodes(long port, unsigned long **inodes) {
    char line[512];
    long count = 0, room = 1024;
    unsigned long *found = malloc((size_t)room * sizeof(*found));
    FILE *table = fopen("/proc/net/tcp", "r");
    if (found == NULL || table == NULL) {
        free(found);
        if (table != NULL) fclose(table);
        return -1;
    }
    while (fgets(line, sizeof(line), table) != NULL) {
        unsigned local_port, state;
        unsigned long inode;
        if (sscanf(line, " %*d: %*8X:%4X %*8X:%*4X %2X %*8X:%*8X %*2X:%*8X %*8X %*u %*d %lu",
                   &local_port, &state, &inode) == 3
            && (long)local_port == port && state == 1) {
            if (count == room) {
                unsigned long *grown;
                room *= 2;
                grown = realloc(found, (size_t)room * sizeof(*found));
                if (grown == NULL) {
                    free(found);
                    fclose(table);
                    return -1;
                }
                found = grown;
            }
            found[count++] = inode;
        }
    }
    fclose(table);
    qsort(found, (size_t)count, sizeof(*found), compare_inodes);
    *inodes = found;
    return count;
}

/* The connections the server holds on `port`: its socket descriptors that are
 * established there. Counting them rather than every descriptor makes the
 * count independent of the server closing its listener after its last
 * accept, as a server started for exactly N connections does. */
static long accepted(long pid, long port) {
    char path[64];
    char target[64];
    long count = 0;
    unsigned long *inodes = NULL;
    long established = established_inodes(port, &inodes);
    DIR *directory;
    struct dirent *entry;
    if (established < 0) {
        return -1;
    }
    snprintf(path, sizeof(path), "/proc/%ld/fd", pid);
    directory = opendir(path);
    if (directory == NULL) {
        free(inodes);
        return -1;
    }
    while ((entry = readdir(directory)) != NULL) {
        char link[320];
        ssize_t length;
        unsigned long inode;
        if (entry->d_name[0] == '.') {
            continue;
        }
        snprintf(link, sizeof(link), "%s/%s", path, entry->d_name);
        length = readlink(link, target, sizeof(target) - 1);
        if (length <= 0) {
            continue;
        }
        target[length] = '\0';
        if (sscanf(target, "socket:[%lu]", &inode) == 1
            && bsearch(&inode, inodes, (size_t)established, sizeof(*inodes), compare_inodes) != NULL) {
            count += 1;
        }
    }
    closedir(directory);
    free(inodes);
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
    long port, count, pid, index, base_rss, base_maps;
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
    base_rss = resident_kib(pid);
    base_maps = mappings(pid);
    if (base_rss < 0 || base_maps < 0) {
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
        long held = accepted(pid, port);
        if (held < 0) {
            fprintf(stderr, "idleload: the server exited before it accepted every connection\n");
            return 1;
        }
        if (held >= count) {
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
