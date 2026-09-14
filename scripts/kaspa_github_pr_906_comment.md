Integrator-side: we need enriched input UTXOs for exchange deposit fee attribution and source-address derivation on owned TN10 nodes.

Our rehearsal crate ([kaspa-frontier-engine](https://github.com/explife365/kaspa-frontier-engine)) currently uses REST chain-walk for return-address (#435) and partial fee estimation when `previous_outpoint_amount` is present.

Happy to test a node build with this PR against our TN10 2/2 gate + deposit journal flow. Evidence pack: https://gist.github.com/explife365/477afea386ddba43574c7cb841ad4c73
