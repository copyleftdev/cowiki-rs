# Reading Notes: Designing Data-Intensive Applications

Martin Kleppmann, 2017. The best systems book written this decade.

Key takeaways by section:

Part I - Foundations:
- The distinction between OLTP and OLAP drives storage engine design (ch 3)
- Log-structured merge trees vs B-trees: LSM for write-heavy, B-tree for read-heavy
- This maps to the Co-Wiki's choice of SQLite (B-tree) for reads vs the flat-file backend for writes

Part II - Distributed Data:
- Replication: leader-based, multi-leader, leaderless. See [[distributed/eventual-consistency]]
- Chapter 5's treatment of replication lag is the clearest I've read
- Partitioning strategies map to the Co-Wiki's namespace/directory structure

Part III - Derived Data:
- Batch processing (MapReduce) vs stream processing (Kafka)
- The lambda architecture (batch + stream) is what the REM agent approximates: periodic batch maintenance (prune, dream) on top of real-time queries

Connections I noticed:
- Kleppmann's "derived data" concept is exactly what the Co-Wiki's TF-IDF index is: derived from the source markdown, rebuilt on change
- The [[distributed/consensus-protocols]] chapter (8) is the clearest explanation of Raft I've found
- The [[distributed/fault-injection]] discussion cites Jepsen extensively

This book changed how I think about [[distributed/eventual-consistency]]. The key insight: eventual consistency is not a bug, it's a feature -- it's the price of availability. Same trade-off the Co-Wiki makes with its mutex: we could be lockless and eventually consistent, or locked and strongly consistent.

See also: [[reading-notes/thinking-fast-and-slow]] for a different angle on the same "two systems" idea.
