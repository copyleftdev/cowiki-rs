.PHONY: demo demo-stop explorer explorer-stop test clean

demo:
	@echo "Building Co-Wiki..."
	@docker build -t cowiki-demo . -q
	@echo "Starting Co-Wiki on http://localhost:3001"
	@docker run --rm -d -p 3001:3001 --name cowiki-demo cowiki-demo > /dev/null
	@sleep 1
	@echo ""
	@echo "  Co-Wiki is running at http://localhost:3001"
	@echo "  20 pages, 92 edges, spreading activation retrieval"
	@echo ""
	@echo "  make demo-stop  to shut down"
	@echo ""
	@which xdg-open > /dev/null 2>&1 && xdg-open http://localhost:3001 || \
	 which open > /dev/null 2>&1 && open http://localhost:3001 || \
	 echo "  Open http://localhost:3001 in your browser"

demo-stop:
	@docker stop cowiki-demo 2>/dev/null || true
	@echo "Co-Wiki stopped."

explorer:
	@echo "Building cowiki-server (release)..."
	@cargo build --release -p cowiki-server
	@echo "Building SCOTUS Explorer UI..."
	@cd ui-scotus && (test -d node_modules || npm install --silent) && npx vite build
	@pgrep -f "cowiki-server.*--port 3002" > /dev/null && kill $$(pgrep -f "cowiki-server.*--port 3002") || true
	@sleep 1
	@echo "Starting SCOTUS Explorer on http://localhost:3002"
	@setsid ./target/release/cowiki-server wiki-corpus/scotus-top10k --port 3002 --ui ui-scotus/dist \
		< /dev/null > /tmp/scotus-explorer.log 2>&1 & disown
	@sleep 2
	@echo ""
	@echo "  SCOTUS Explorer: http://localhost:3002"
	@echo "  Demo (untouched): http://localhost:3001"
	@echo ""
	@echo "  make explorer-stop  to shut down"
	@echo ""
	@which xdg-open > /dev/null 2>&1 && xdg-open http://localhost:3002 || \
	 which open > /dev/null 2>&1 && open http://localhost:3002 || \
	 echo "  Open http://localhost:3002 in your browser"

explorer-stop:
	@pgrep -f "cowiki-server.*--port 3002" > /dev/null \
		&& kill $$(pgrep -f "cowiki-server.*--port 3002") \
		&& echo "SCOTUS Explorer stopped." \
		|| echo "SCOTUS Explorer not running."

test:
	cargo test

clean:
	cargo clean
	@docker rmi cowiki-demo 2>/dev/null || true
