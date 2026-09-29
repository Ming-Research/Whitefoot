//! Shared cycle decomposition for code specialization, the stack ledger and
//! call-offer grain.

/// Iterative Tarjan decomposition. Edges must index the supplied node table.
/// Component order is deterministic and places callees before their callers.
pub(crate) fn components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let count = edges.len();
    let mut order = vec![usize::MAX; count];
    let mut low = vec![0_usize; count];
    let mut on_stack = vec![false; count];
    let mut stack: Vec<usize> = Vec::new();
    let mut frames: Vec<(usize, usize)> = Vec::new();
    let mut next_order = 0_usize;
    let mut found: Vec<Vec<usize>> = Vec::new();
    for root in 0..count {
        if order[root] != usize::MAX {
            continue;
        }
        order[root] = next_order;
        low[root] = next_order;
        next_order += 1;
        stack.push(root);
        on_stack[root] = true;
        frames.push((root, 0));
        while let Some((node, cursor)) = frames.last_mut() {
            let node = *node;
            if let Some(target) = edges[node].get(*cursor).copied() {
                *cursor += 1;
                if order[target] == usize::MAX {
                    order[target] = next_order;
                    low[target] = next_order;
                    next_order += 1;
                    stack.push(target);
                    on_stack[target] = true;
                    frames.push((target, 0));
                } else if on_stack[target] {
                    low[node] = low[node].min(order[target]);
                }
                continue;
            }
            frames.pop();
            if let Some((parent, _)) = frames.last() {
                low[*parent] = low[*parent].min(low[node]);
            }
            if low[node] == order[node] {
                let mut component = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                found.push(component);
            }
        }
    }
    found
}
