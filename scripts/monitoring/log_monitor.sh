#!/bin/bash
while true; do
    clear
    echo "=== ShrivenQuant Paper Trading Monitor ==="
    echo "Time: $(date)"
    echo ""
    echo "Service Status:"
    for pid_file in logs/*.pid; do
        if [ -f "$pid_file" ]; then
            service=$(basename $pid_file .pid)
            pid=$(cat $pid_file)
            if kill -0 $pid 2>/dev/null; then
                echo "  ✅ $service (PID: $pid)"
            else
                echo "  ❌ $service (STOPPED)"
            fi
        fi
    done
    echo ""
    echo "Recent Trading Activity:"
    tail -5 logs/paper_trading.log | grep -E "TRADE|ORDER" || echo "  No recent trades"
    echo ""
    echo "Errors (last hour):"
    find logs -name "*.log" -mmin -60 -exec grep -l ERROR {} \; | wc -l | xargs echo "  Error count:"
    echo ""
    echo "Press Ctrl+C to exit monitoring"
    sleep 10
done
