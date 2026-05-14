#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0

# Check commits in BASE..HEAD.
# PGP signatures are verified locally.
# SSH signatures are only detected; GitHub branch protection verifies trust.

set -euo pipefail

BASE=${1:-origin/master}
RANGE="$BASE..HEAD"

TOTAL=0
FAILED=0

for COMMIT in $(git rev-list "$RANGE"); do
    TOTAL=$((TOTAL + 1))

    # Signed commits contain a gpgsig header in the raw commit object.
    SIGNATURE_HEADER=$(
        git cat-file commit "$COMMIT" |
            grep -m1 -E '^gpgsig(-sha256)? ' ||
            true
    )

    echo "Checking $(git show -s --format='%H %an <%ae> %s' "$COMMIT")"
    echo "Signature header: ${SIGNATURE_HEADER:-none}"

    case "$SIGNATURE_HEADER" in
        *"BEGIN PGP SIGNATURE"*)
            if ! git verify-commit "$COMMIT" >/dev/null 2>&1; then
                echo "❌ Invalid or untrusted PGP signature: $COMMIT"
                FAILED=$((FAILED + 1))
            fi
            ;;

        *"BEGIN SSH SIGNATURE"*)
            # Present. GitHub verifies whether the SSH signer is trusted.
            ;;

        "")
            echo "❌ Unsigned commit: $COMMIT"
            FAILED=$((FAILED + 1))
            ;;

        *)
            echo "❌ Unknown signature type: $COMMIT"
            FAILED=$((FAILED + 1))
            ;;
    esac
done

if [ "$FAILED" -gt 0 ]; then
    echo "⚠️  Commit signature check failed [$FAILED/$TOTAL]"
    exit 1
fi

echo "🔏 Commit signature check passed [$TOTAL/$TOTAL]"
