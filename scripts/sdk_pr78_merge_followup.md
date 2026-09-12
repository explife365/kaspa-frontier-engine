# kaspa-python-sdk #78 merge follow-up (TN10 integrator)

Posted to [#79](https://github.com/kaspanet/kaspa-python-sdk/issues/79#issuecomment-5643469862) on 12 Sep 2026.

## Wheel poll

```bash
python scripts/tn10_sdk_wheel_watch.py --once --json
```

| Field | Value |
|-------|-------|
| PyPI `kaspa` | 2.0.1 |
| Installed | 2.0.2rc1 |
| `readyNative` | false |
| PR #78 | MERGED |

## TN10 shipped (dev patch)

- 8 covenant fixtures offline-verified
- Adoption gate 2/2 green
- Evidence: `.local/evidence/evidence_20260911-234059.txt`

## Unblock

Publish wheel with `computeBudget` in `TransactionInput.to_dict()`, then:

```powershell
powershell -File scripts/tn10_fixture_publish_when_ready.ps1
```
