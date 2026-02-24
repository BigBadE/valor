#!/bin/bash
cd /home/ethan/projects/valor

for f in crates/page/tests/fixtures/*.html; do
    name=""
    mkdir -p /tmp/fixture_test
    rm -f /tmp/fixture_test/*.html
    cp "" /tmp/fixture_test/
    
    mv crates/page/tests/fixtures crates/page/tests/fixtures_real
    mv /tmp/fixture_test crates/page/tests/fixtures
    
    result=timeout: failed to run command ‘cargo’: No such file or directory
    
    mv crates/page/tests/fixtures /tmp/fixture_test
    mv crates/page/tests/fixtures_real crates/page/tests/fixtures
    
    if echo "" | grep -q "stack overflow"; then
        echo "OVERFLOW: "
    else
        echo "OK: "
    fi
done
