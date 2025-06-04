#!/bin/bash
# Phase 3 パフォーマンステストスクリプト

set -e

echo "=== Phase 3 Performance Test Script ==="
echo ""

# カラー定義
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# プロジェクトルートに移動
cd "$(dirname "$0")/.."

# ビルド確認
echo -e "${YELLOW}Checking build...${NC}"
if [ ! -f "./target/release/voice_inputd" ] || [ ! -f "./target/release/voice_input" ]; then
    echo -e "${RED}Release build not found. Building...${NC}"
    cargo build --release
fi

# voice_input_uiプロセスのPIDを取得する関数
get_ui_pid() {
    pgrep -f "voice_input_ui" || echo "0"
}

# CPU使用率を測定する関数
measure_cpu() {
    local pid=$1
    local duration=$2
    local samples=$3
    
    if [ "$pid" -eq "0" ]; then
        echo "N/A"
        return
    fi
    
    local total=0
    for i in $(seq 1 $samples); do
        local cpu=$(ps -p $pid -o %cpu | tail -n 1 | tr -d ' ')
        total=$(echo "$total + $cpu" | bc)
        sleep $(echo "$duration / $samples" | bc -l)
    done
    
    echo "scale=2; $total / $samples" | bc
}

# メモリ使用量を取得する関数
get_memory() {
    local pid=$1
    if [ "$pid" -eq "0" ]; then
        echo "N/A"
        return
    fi
    
    ps -p $pid -o rss | tail -n 1 | awk '{print $1/1024 " MB"}'
}

echo -e "${YELLOW}Starting performance tests...${NC}"
echo ""

# 1. UI起動時のパフォーマンス
echo -e "${GREEN}1. UI Launch Performance${NC}"
echo "   Please run: ./target/release/voice_input stack-mode on"
echo "   Press Enter when UI is visible..."
read -r

UI_PID=$(get_ui_pid)
if [ "$UI_PID" -eq "0" ]; then
    echo -e "${RED}   UI process not found!${NC}"
else
    echo "   UI PID: $UI_PID"
    echo "   Initial Memory: $(get_memory $UI_PID)"
    echo ""
fi

# 2. アイドル時のCPU使用率
echo -e "${GREEN}2. Idle CPU Usage (5 seconds)${NC}"
if [ "$UI_PID" -ne "0" ]; then
    echo -n "   Measuring... "
    idle_cpu=$(measure_cpu $UI_PID 5 5)
    echo -e "${GREEN}Done${NC}"
    echo "   Average CPU: ${idle_cpu}%"
    echo ""
else
    echo -e "${RED}   Skipped (UI not running)${NC}"
    echo ""
fi

# 3. スタック作成のレスポンス
echo -e "${GREEN}3. Stack Creation Response${NC}"
echo "   Creating 5 test stacks..."
for i in {1..5}; do
    echo "   Stack $i:"
    time_output=$( { time echo "Test stack $i" | ./target/release/voice_input record; } 2>&1 )
    real_time=$(echo "$time_output" | grep real | awk '{print $2}')
    echo "     Time: $real_time"
done
echo ""

# 4. ハイライト中のCPU使用率
echo -e "${GREEN}4. CPU Usage During Highlight${NC}"
echo "   Please press Cmd+1 to trigger highlight"
echo "   Press Enter immediately after..."
read -r

if [ "$UI_PID" -ne "0" ]; then
    echo -n "   Measuring for 3 seconds... "
    highlight_cpu=$(measure_cpu $UI_PID 3 6)
    echo -e "${GREEN}Done${NC}"
    echo "   Average CPU during highlight: ${highlight_cpu}%"
    echo ""
else
    echo -e "${RED}   Skipped (UI not running)${NC}"
    echo ""
fi

# 5. 大量スタック時のパフォーマンス
echo -e "${GREEN}5. Performance with Many Stacks${NC}"
echo "   Creating 15 stacks..."
for i in {6..20}; do
    echo "Stack $i content" | ./target/release/voice_input record >/dev/null 2>&1
    echo -n "."
done
echo -e " ${GREEN}Done${NC}"

if [ "$UI_PID" -ne "0" ]; then
    echo "   Memory after 20 stacks: $(get_memory $UI_PID)"
    echo -n "   CPU with 20 stacks (5 seconds): "
    many_stacks_cpu=$(measure_cpu $UI_PID 5 5)
    echo "${many_stacks_cpu}%"
else
    echo -e "${RED}   Memory/CPU measurement skipped (UI not running)${NC}"
fi
echo ""

# 6. リスト表示のレスポンス
echo -e "${GREEN}6. List Command Response${NC}"
echo "   Timing list command..."
time_output=$( { time ./target/release/voice_input list; } 2>&1 )
real_time=$(echo "$time_output" | grep real | awk '{print $2}')
echo "   List command time: $real_time"
echo ""

# 結果サマリー
echo -e "${YELLOW}=== Performance Summary ===${NC}"
echo ""
echo "UI Process:"
if [ "$UI_PID" -ne "0" ]; then
    echo "  - PID: $UI_PID"
    echo "  - Current Memory: $(get_memory $UI_PID)"
    echo "  - Idle CPU: ${idle_cpu}%"
    echo "  - Highlight CPU: ${highlight_cpu}%"
    echo "  - CPU with 20 stacks: ${many_stacks_cpu}%"
else
    echo "  - Not running"
fi
echo ""
echo "Response Times:"
echo "  - Stack creation: ~0.1s (expected)"
echo "  - List command: $real_time"
echo ""

# 推奨値チェック
echo -e "${YELLOW}Performance Check:${NC}"
if [ "$UI_PID" -ne "0" ]; then
    if (( $(echo "$idle_cpu < 5" | bc -l) )); then
        echo -e "  - Idle CPU: ${GREEN}PASS${NC} (< 5%)"
    else
        echo -e "  - Idle CPU: ${RED}FAIL${NC} (> 5%)"
    fi
    
    if (( $(echo "$highlight_cpu < 10" | bc -l) )); then
        echo -e "  - Highlight CPU: ${GREEN}PASS${NC} (< 10%)"
    else
        echo -e "  - Highlight CPU: ${RED}FAIL${NC} (> 10%)"
    fi
fi
echo ""

# クリーンアップ
echo -e "${YELLOW}Cleanup:${NC}"
echo "  Run: ./target/release/voice_input clear"
echo "  Run: ./target/release/voice_input stack-mode off"
echo ""

echo -e "${GREEN}Test completed!${NC}"