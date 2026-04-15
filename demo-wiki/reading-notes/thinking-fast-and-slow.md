# Reading Notes: Thinking, Fast and Slow

Daniel Kahneman, 2011. The dual-process theory of cognition.

System 1: fast, automatic, associative. Handles pattern matching, emotional reactions, and intuitive judgments. This is [[cognitive/spreading-activation]] -- it's the mechanism by which System 1 makes associations.

System 2: slow, deliberate, logical. Handles complex reasoning, planning, and effortful computation. This is what you're doing when you explicitly search a wiki instead of letting activation spread naturally.

The Co-Wiki is designed to augment System 1. By externalizing your associative network into a wiki with backlinks, you give System 1 a larger memory to spread activation through. The [[systems/rem-agent]] maintains this external memory the way biological sleep maintains internal memory (see [[cognitive/memory-consolidation]]).

Key concepts:
- Anchoring: initial activation biases subsequent judgments. The a0 ignition vector is literally an anchor.
- Availability heuristic: we judge probability by how easily examples come to mind. The Co-Wiki makes more examples "available" through backlink traversal.
- Associative coherence: System 1 automatically builds coherent stories from activated concepts. This is what spreading activation does on the knowledge graph.

[[cognitive/priming]] is the experimental evidence for System 1's associative nature. Kahneman cites the priming literature extensively.

The "two systems" framework parallels Kleppmann's batch vs stream processing in [[reading-notes/designing-data-intensive-applications]]. System 1 = stream (real-time, approximate). System 2 = batch (slow, precise).
