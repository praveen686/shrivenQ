#!/bin/bash

# Script to replace magic numbers with named constants
# This fixes the compliance issue of having 400+ magic numbers

echo "Fixing magic numbers in Rust files..."

# Add imports to all service files that need constants
for file in $(find services/ -name "*.rs" -type f | grep -v test); do
    # Check if file uses 10000 and doesn't already import constants
    if grep -q "10000" "$file" && ! grep -q "use services_common::constants" "$file"; then
        # Add import after the first use statement or at the beginning
        if grep -q "^use " "$file"; then
            # Add after first use statement
            sed -i '0,/^use /{s/^use /use services_common::constants::*;\nuse /}' "$file"
        else
            # Add at beginning after module doc comments
            sed -i '1s/^/use services_common::constants::*;\n\n/' "$file"
        fi
    fi
done

# Replace all instances of 10000 with FIXED_POINT_SCALE
find services/ -name "*.rs" -type f | xargs sed -i 's/\b10000\b/FIXED_POINT_SCALE/g'
find services/ -name "*.rs" -type f | xargs sed -i 's/10000\.0/FIXED_POINT_SCALE_F64/g'
find services/ -name "*.rs" -type f | xargs sed -i 's/10_000/FIXED_POINT_SCALE/g'

# Replace time constants
find services/ -name "*.rs" -type f | xargs sed -i 's/\b3600\b/SECS_PER_HOUR/g'
find services/ -name "*.rs" -type f | xargs sed -i 's/\b86400\b/SECS_PER_DAY/g'
find services/ -name "*.rs" -type f | xargs sed -i 's/\b1000000000\b/NANOS_PER_SEC/g'
find services/ -name "*.rs" -type f | xargs sed -i 's/\b1_000_000_000\b/NANOS_PER_SEC/g'
find services/ -name "*.rs" -type f | xargs sed -i 's/\b1000000\b/MICROS_PER_SEC/g'
find services/ -name "*.rs" -type f | xargs sed -i 's/\b1_000_000\b/MICROS_PER_SEC/g'

# Replace size constants
find services/ -name "*.rs" -type f | xargs sed -i 's/\b1024\b/BYTES_PER_KB/g'
find services/ -name "*.rs" -type f | xargs sed -i 's/1024 \* 1024/BYTES_PER_MB/g'
find services/ -name "*.rs" -type f | xargs sed -i 's/1024 \* BYTES_PER_KB/BYTES_PER_MB/g'

# Replace common buffer/channel sizes
find services/ -name "*.rs" -type f | xargs sed -i 's/channel(\b1000\b)/channel(DEFAULT_CHANNEL_SIZE)/g'
find services/ -name "*.rs" -type f | xargs sed -i 's/with_capacity(\b1000\b)/with_capacity(DEFAULT_CHANNEL_SIZE)/g'

echo "Magic numbers replacement complete!"
echo "Note: Some replacements may need manual review for context-specific uses."