#!/bin/bash
# A2UI Demo Script
# Run these examples to see A2UI surfaces in the main chat

export OQTO_SESSION_ID="${OQTO_SESSION_ID:-main-chat}"
export OQTO_API_URL="${OQTO_API_URL:-http://localhost:8080}"

cd "$(dirname "$0")/.." || exit 1

echo "A2UI Demo - sending surfaces to session: $OQTO_SESSION_ID"
echo ""

demo_simple_button() {
    echo "=== Simple Button ==="
    cargo run --bin oqtoctl -q -- a2ui button "Deploy to production?" --options "Deploy,Cancel"
}

demo_text_input() {
    echo "=== Text Input ==="
    cargo run --bin oqtoctl -q -- a2ui input "Enter your commit message" --type text --placeholder "feat: ..."
}

demo_choice() {
    echo "=== Multiple Choice ==="
    cargo run --bin oqtoctl -q -- a2ui choice "Select environment" --options "Development,Staging,Production"
}

demo_checkbox() {
    echo "=== Checkbox Toggle ==="
    cargo run --bin oqtoctl -q -- a2ui checkbox "Enable verbose logging"
}

demo_slider() {
    echo "=== Slider ==="
    cargo run --bin oqtoctl -q -- a2ui slider "Concurrency level" --min 1 --max 16 --default 4
}

demo_code_review() {
    echo "=== Code Review Form ==="
    cargo run --bin oqtoctl -q -- a2ui raw '[
        {"type":"text","content":"## Code Review: PR #127\n\n**Title:** Add user authentication\n**Author:** @alice\n**Files changed:** 12"},
        {"type":"divider"},
        {"type":"text","content":"### Your Review"},
        {"type":"multipleChoice","id":"decision","options":[{"value":"approve","label":"Approve"},{"value":"request_changes","label":"Request Changes"},{"value":"comment","label":"Comment Only"}],"maxAllowedSelections":1,"selections":{"path":"/decision"}},
        {"type":"textField","id":"comment","placeholder":"Add your review comments...","value":{"path":"/comment"}},
        {"type":"button","id":"btn_submit","label":"Submit Review","action":"submit_review","variant":"primary"}
    ]'
}

demo_deploy_config() {
    echo "=== Deployment Configuration ==="
    cargo run --bin oqtoctl -q -- a2ui raw '[
        {"type":"text","content":"## Deploy Configuration"},
        {"type":"multipleChoice","id":"env","options":[{"value":"dev","label":"Development"},{"value":"staging","label":"Staging"},{"value":"prod","label":"Production"}],"maxAllowedSelections":1,"selections":{"path":"/env"}},
        {"type":"checkBox","id":"migrate","label":"Run database migrations","value":{"path":"/migrate"}},
        {"type":"checkBox","id":"backup","label":"Create backup before deploy","value":{"path":"/backup"}},
        {"type":"slider","id":"replicas","min":1,"max":10,"step":1,"value":{"path":"/replicas"}},
        {"type":"button","id":"deploy","label":"Deploy Now","action":"deploy","variant":"primary"}
    ]'
}

demo_task_selection() {
    echo "=== Task Prioritization ==="
    cargo run --bin oqtoctl -q -- a2ui raw '[
        {"type":"text","content":"## Sprint Planning\n\nSelect tasks for this sprint (max 3):"},
        {"type":"multipleChoice","id":"tasks","options":[{"value":"auth","label":"Implement OAuth login"},{"value":"api","label":"Add REST API endpoints"},{"value":"tests","label":"Write unit tests"},{"value":"docs","label":"Update documentation"},{"value":"bugs","label":"Fix reported bugs"}],"maxAllowedSelections":3,"selections":{"path":"/tasks"}},
        {"type":"button","id":"create","label":"Create Sprint","action":"create_sprint","variant":"primary"}
    ]'
}

# Run demos
case "${1:-all}" in
    button) demo_simple_button ;;
    input) demo_text_input ;;
    choice) demo_choice ;;
    checkbox) demo_checkbox ;;
    slider) demo_slider ;;
    review) demo_code_review ;;
    deploy) demo_deploy_config ;;
    tasks) demo_task_selection ;;
    all)
        demo_simple_button
        sleep 1
        demo_text_input
        sleep 1
        demo_choice
        sleep 1
        demo_checkbox
        sleep 1
        demo_slider
        ;;
    *)
        echo "Usage: $0 [button|input|choice|checkbox|slider|review|deploy|tasks|all]"
        exit 1
        ;;
esac

echo ""
echo "Check the main chat to see the A2UI surfaces!"
