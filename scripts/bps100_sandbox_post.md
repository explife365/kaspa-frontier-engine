Independent TN10 integrator notes — 100 BPS packing sandbox (not activation)

This is **not** a KIP, not a kaspad patch, and not a homemade testnet.

Live L1 is GHOSTDAG @ 10 BPS with k=18 (Crescendo). 100 BPS is kaspa.org lore for a later hard fork. [KIP-2 / DAGKnight](https://github.com/kaspanet/kips/blob/master/kip-0002.md) remains **Status: Proposed**.

BPS is blocks per second, not TPS. The toy question is: in a delay window of about `L` seconds, roughly `bps × L` honest blocks can land together. GHOSTDAG’s k-cluster bound covers that window iff occupancy ≤ k.

Method: in-process virtual miners, Poisson-like emits, one observer with a 1s delay window. Not kaspad. Not DAGKnight. Four miners, 20s, L=1s.

## Occupancy vs live k=18

| Target | Blocks | Max occupancy | Mean | P(>k) | Covers? |
| --- | ---: | ---: | ---: | ---: | --- |
| 10 BPS (live), k=18 | 200 | 14 | 10.2 | 0 | yes (slack) |
| 100 BPS (lore), k=18 | 2001 | 115 | 97.6 | ≈0.99 | no |

## SAT packing of `bps × L ≤ k`

Arithmetic and solver agree:

| BPS | k | Result |
| ---: | ---: | --- |
| 10 | 18 | SAT |
| 100 | 18 | UNSAT (k+1 pigeons into 18 holes) |
| 100 | 100 | SAT |

## Delay jitter: k = BPS is not enough

Exact packing says 100 BPS needs k ≥ 100 at L=1s. Occupancy overshoots because one-way delay is not a constant L.

At 100 BPS, the same 2001-block trace:

| k | P(occupancy > k) | Covers? |
| ---: | ---: | --- |
| 18 | 0.991 | no |
| 50 | 0.975 | no |
| 100 | 0.458 | no (max was 115) |
| 128 | 0 | yes |

So “just set k = BPS” fails this toy if the window has delay variance.

## Takeaway for integrators

- Today’s rehearsal target is **10 BPS with k=18 slack**. That is live.
- 100 BPS does not fit today’s k. A different k (research) or DAGKnight (KIP-2) is Core’s path: applied research in the KIP, implementation in rusty-kaspa, staged nets, `kip-0002.md` leaving Proposed, then a mainnet HF.
- Independent SAT/occupancy does not mint that. Do not patch kaspad BPS, do not IBD a homemade 100 BPS net, do not print 100 as live DAA/s.

Happy to answer questions here. No node RPC/P2P ports or private rehearsal hosts in this note.
