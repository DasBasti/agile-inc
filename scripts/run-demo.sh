#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

export PATH="$HOME/.cargo/bin:$PATH"

MQTT_HOST="${MQTT_HOST:-localhost}"
MQTT_PORT="${MQTT_PORT:-1883}"

show_usage() {
    echo "Usage: $0 [options]"
    echo ""
    echo "Options:"
    echo "  --title <title>         Story title (required for automated mode)"
    echo "  --body <body>          Story body/description"
    echo "  --criteria <c1,c2...>  Acceptance criteria (comma-separated)"
    echo "  --dod <g1,g2...>       Definition of done gates (comma-separated)"
    echo "  --auto                 Run in automated mode without waiting for input"
    echo "  --po-run               Trigger po.run event for direct agent testing"
    echo "  --prompt <text>        Prompt for po.run mode"
    echo "  --no-flush             Don't flush Redis/MQTT before starting"
    echo "  --help                 Show this help"
    echo ""
    echo "If run without --auto, the script will start agents and wait for"
    echo "you to enter a story prompt interactively."
}

TITLE=""
BODY=""
CRITERIA=""
DOD=""
AUTO_MODE=false
PO_RUN=false
PROMPT=""
FLUSH=true

while [[ $# -gt 0 ]]; do
    case $1 in
        --title)
            TITLE="$2"
            shift 2
            ;;
        --body)
            BODY="$2"
            shift 2
            ;;
        --criteria)
            CRITERIA="$2"
            shift 2
            ;;
        --dod)
            DOD="$2"
            shift 2
            ;;
        --no-flush)
            FLUSH=false
            shift
            ;;
        --auto)
            AUTO_MODE=true
            shift
            ;;
        --po-run)
            PO_RUN=true
            shift
            ;;
        --prompt)
            PROMPT="$2"
            shift 2
            ;;
        --help)
            show_usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

cleanup() {
    echo ""
    echo "Stopping agents..."
    jobs -p | xargs -r kill 2>/dev/null || true
    wait 2>/dev/null || true
}
trap cleanup EXIT

wait_for_audit_tool() {
    echo "Waiting for audit tool to be ready..."
    local max_attempts=30
    local attempt=1
    while [ $attempt -le $max_attempts ]; do
        if curl -s http://localhost:3000 > /dev/null 2>&1; then
            echo "Audit tool is ready!"
            return 0
        fi
        echo "  Attempt $attempt/$max_attempts..."
        sleep 1
        attempt=$((attempt + 1))
    done
    echo "Warning: Audit tool did not respond in time"
    return 1
}

flush_data() {
    echo "Flushing Redis and MQTT..."
    docker exec infra-redis-1 redis-cli FLUSHALL > /dev/null 2>&1 || true
    docker exec infra-mqtt-1 mosquitto_pub -t "agileinc/flush" -m "flush" > /dev/null 2>&1 || true
    echo "Data flushed."
}

echo "Starting Agile Inc. agents..."
echo "MQTT: $MQTT_HOST:$MQTT_PORT"
echo ""

cd "$PROJECT_DIR"

if [ "$FLUSH" = true ]; then
    flush_data
fi

echo "Starting audit tool..."
cargo run -p tools-audit-tool --release > /tmp/audit-tool.log 2>&1 &
AUDIT_PID=$!
sleep 3

wait_for_audit_tool

echo ""
echo "Starting PO agent..."
cargo run -p agents-po --bin po --release > /tmp/agent-po.log 2>&1 &
PO_PID=$!
sleep 2

echo "Starting Dev agent..."
cargo run -p agents-dev --bin dev --release > /tmp/agent-dev.log 2>&1 &
DEV_PID=$!
sleep 2

echo "Starting QA agent..."
cargo run -p agents-qa --bin qa --release > /tmp/agent-qa.log 2>&1 &
QA_PID=$!
sleep 2

echo ""
echo "Agents started:"
echo "  PO:   $PO_PID"
echo "  Dev:  $DEV_PID"
echo "  QA:   $QA_PID"
echo "  Audit: $AUDIT_PID"
echo ""
echo "Audit tool: http://localhost:3000"
echo ""

if [ "$PO_RUN" = true ]; then
    if [ -z "$PROMPT" ]; then
        echo "Error: --prompt is required for po.run mode"
        exit 1
    fi
    
    echo "Running po.run mode..."
    echo "Prompt: $PROMPT"
    
    cargo run -p tools_publish_event --bin publish_event --release -- \
        --po-run \
        --prompt "$PROMPT" \
        --mqtt-host "$MQTT_HOST" \
        --mqtt-port "$MQTT_PORT"
    
    echo ""
    echo "po.run submitted! Check the audit tool for progress."
    echo "Press Ctrl+C to stop all agents."
    
    wait
elif [ "$AUTO_MODE" = true ]; then
    if [ -z "$TITLE" ]; then
        echo "Error: --title is required in auto mode"
        exit 1
    fi
    
    echo "Running in automated mode..."
    echo "Submitting story: $TITLE"
    
    CRITERIA_ARG=""
    if [ -n "$CRITERIA" ]; then
        CRITERIA_ARG="--criteria $CRITERIA"
    fi
    
    DOD_ARG=""
    if [ -n "$DOD" ]; then
        DOD_ARG="--dod $DOD"
    fi
    
    cargo run -p tools_publish_event --bin publish_event --release -- \
        --title "$TITLE" \
        --body "${BODY:-Demo story}" \
        $CRITERIA_ARG \
        $DOD_ARG \
        --mqtt-host "$MQTT_HOST" \
        --mqtt-port "$MQTT_PORT"
    
    echo ""
    echo "Story submitted! Check the audit tool for progress."
    echo "Press Ctrl+C to stop all agents."
    
    wait
else
    echo "Enter a story prompt (or press Enter for demo story):"
    read -r user_input
    
    if [ -z "$user_input" ]; then
        echo "Using demo story..."
        cargo run -p tools_publish_event --bin publish_event --release -- \
            --demo \
            --mqtt-host "$MQTT_HOST" \
            --mqtt-port "$MQTT_PORT"
    else
        TITLE="$user_input"
        cargo run -p tools_publish_event --bin publish_event --release -- \
            --title "$TITLE" \
            --body "User submitted story: $TITLE" \
            --mqtt-host "$MQTT_HOST" \
            --mqtt-port "$MQTT_PORT"
    fi
    
    echo ""
    echo "Story submitted! Check the audit tool for progress."
    echo "Press Ctrl+C to stop all agents."
    
    wait
fi
