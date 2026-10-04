#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct Node { struct Node *left, *right; } Node;

static Node *tree(unsigned depth) {
    Node *node = malloc(sizeof *node);
    if (!node) { fputs("allocation failed\n", stderr); exit(1); }
    node->left = depth ? tree(depth - 1) : NULL;
    node->right = depth ? tree(depth - 1) : NULL;
    return node;
}
static uint64_t check(const Node *node) {
    return node ? 1 + check(node->left) + check(node->right) : 0;
}
static void discard(Node *node) {
    if (node) { discard(node->left); discard(node->right); free(node); }
}
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    char *end;
    unsigned long input = strtoul(argv[1], &end, 10);
    if (*end || input < 4 || input > 16) return 2;
    unsigned n = (unsigned)input;
    Node *stretch = tree(n + 1);
    printf("stretch tree of depth %u\t check: %llu\n", n + 1,
           (unsigned long long)check(stretch));
    discard(stretch);
    Node *long_lived = tree(n);
    for (unsigned d = 4; d <= n; d += 2) {
        uint64_t iterations = UINT64_C(1) << (n - d + 4), sum = 0;
        for (uint64_t i = 0; i < iterations; ++i) {
            Node *temporary = tree(d);
            sum += check(temporary);
            discard(temporary);
        }
        printf("%llu\t trees of depth %u\t check: %llu\n",
               (unsigned long long)iterations, d, (unsigned long long)sum);
    }
    printf("long lived tree of depth %u\t check: %llu\n", n,
           (unsigned long long)check(long_lived));
    discard(long_lived);
    return 0;
}
