cargo check -p codexmanager-core -p codexmanager-service --message-format=short > .tmp-cargo-check.out 2> .tmp-cargo-check.err
echo EXIT:%ERRORLEVEL%>> .tmp-cargo-check.err
