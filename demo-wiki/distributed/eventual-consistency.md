# Eventual Consistency

A consistency model where, if no new updates are made, all replicas will eventually converge to the same value. Contrast with strong consistency (linearizability) where reads always see the latest write.

The CAP theorem forces a choice: during a network partition, you either sacrifice consistency (AP) or availability (CP). Most real systems choose eventual consistency because availability matters more than instantaneous agreement.

Practical implications:
- Reads may return stale data
- Concurrent writes need conflict resolution (last-writer-wins, CRDTs, application-level merge)
- Causal ordering is often sufficient (you don't need total order)

The Co-Wiki's persistence layer has a version of this problem. When the [[systems/rem-agent]] modifies the graph while a query is in flight, we use a mutex. But Shomo's manifesto suggests the wiki should eventually support concurrent human and agent edits, which is an optimistic concurrency model -- essentially eventual consistency for wiki pages.

Related: [[distributed/consensus-protocols]] (the alternative to eventual consistency), [[distributed/fault-injection]] (testing behavior under partition).

I'm applying this thinking to a side project. See [[reading-notes/designing-data-intensive-applications]] chapters 5 and 9.
