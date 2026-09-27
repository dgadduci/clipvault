# QuickVault live capture ordering

This change makes QuickVault's recent list reflect newly persisted local
captures and successful peer imports immediately, in strict newest-first
capture-time order. It also prevents an older overlapping refresh from
replacing newer results.
