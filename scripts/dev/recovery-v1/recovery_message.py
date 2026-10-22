"""Fixed actionable diagnostic shared by heavy and its landing capture."""
RECOVERY_DIAGNOSTIC = (
    'heavy: recovery/storage failure; task ownership is retained. Free space, restore/restart '
    'the SAME recovery service if needed, then run heavy --reconcile with the SAME queue and '
    'DEV_TMP state. RETAIN the ledger; do not delete/reset it. Rerun land if completion is '
    'recovered with exit 137.')
