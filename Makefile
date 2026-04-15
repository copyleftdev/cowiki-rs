.PHONY: demo demo-stop test clean

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

test:
	cargo test

clean:
	cargo clean
	@docker rmi cowiki-demo 2>/dev/null || true
