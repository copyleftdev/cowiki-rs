import { useState, useEffect, useCallback } from 'react'
import {
  Box, Flex, Text, Heading, TextField, Button, Card, Badge,
  Separator, ScrollArea, TextArea, IconButton, Container,
  DataList, Em, Strong, Code, Callout,
} from '@radix-ui/themes'
import * as Tabs from '@radix-ui/react-tabs'
import * as Dialog from '@radix-ui/react-dialog'
import { listPages, getPage, queryPages, createPage, runMaintain, getStats } from './api'

export default function App() {
  const [pages, setPages] = useState([])
  const [selected, setSelected] = useState(null)
  const [query, setQuery] = useState('')
  const [results, setResults] = useState(null)
  const [stats, setStats] = useState(null)
  const [maintainResult, setMaintainResult] = useState(null)
  const [loading, setLoading] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  const [newId, setNewId] = useState('')
  const [newTitle, setNewTitle] = useState('')
  const [newContent, setNewContent] = useState('')

  const refresh = useCallback(async () => {
    const [p, s] = await Promise.all([listPages(), getStats()])
    setPages(p)
    setStats(s)
  }, [])

  useEffect(() => { refresh() }, [refresh])

  const handleQuery = async () => {
    if (!query.trim()) return
    setLoading(true)
    const r = await queryPages(query)
    setResults(r)
    setLoading(false)
  }

  const handleSelect = async (id) => {
    const p = await getPage(id)
    setSelected(p)
  }

  const handleMaintain = async () => {
    setLoading(true)
    const r = await runMaintain()
    setMaintainResult(r)
    await refresh()
    setLoading(false)
  }

  const handleCreate = async () => {
    if (!newId.trim() || !newTitle.trim()) return
    await createPage(newId, newTitle, newContent)
    setCreateOpen(false)
    setNewId('')
    setNewTitle('')
    setNewContent('')
    await refresh()
  }

  const renderBacklinks = (content) => {
    if (!content) return null
    return content.replace(/\[\[([^\]]+)\]\]/g, (_, target) => {
      return `[${target}]`
    })
  }

  return (
    <Container size="4" p="4">
      <Flex direction="column" gap="4">
        <Flex justify="between" align="center">
          <Box>
            <Heading size="6">Co-Wiki</Heading>
            <Text size="2" color="gray">
              Spreading activation retrieval engine
            </Text>
          </Box>
          <Flex gap="2" align="center">
            {stats && (
              <Flex gap="3">
                <Badge variant="soft" color="cyan">{stats.page_count} pages</Badge>
                <Badge variant="soft" color="orange">{stats.edge_count} edges</Badge>
                <Badge variant="soft" color="purple">
                  {(stats.density * 100).toFixed(1)}% density
                </Badge>
              </Flex>
            )}
          </Flex>
        </Flex>

        <Separator size="4" />

        <Flex gap="4" style={{ minHeight: '70vh' }}>
          {/* Left panel: Search + Page list */}
          <Box style={{ width: '340px', flexShrink: 0 }}>
            <Flex direction="column" gap="3">
              <Flex gap="2">
                <Box style={{ flex: 1 }}>
                  <TextField.Root
                    placeholder="Query the wiki..."
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && handleQuery()}
                  />
                </Box>
                <Button onClick={handleQuery} disabled={loading}>
                  Search
                </Button>
              </Flex>

              {results && (
                <Callout.Root size="1" color="cyan">
                  <Callout.Text>
                    {results.pages.length} results | score: {results.total_score.toFixed(3)} |
                    {' '}{results.iterations} iterations |
                    {' '}{results.converged ? 'converged' : 'max iter'}
                  </Callout.Text>
                </Callout.Root>
              )}

              {/* Query results */}
              {results && results.pages.length > 0 && (
                <Box>
                  <Text size="2" weight="bold" color="gray">Results</Text>
                  <Flex direction="column" gap="1" mt="1">
                    {results.pages.map((p) => (
                      <Card
                        key={p.id}
                        style={{ cursor: 'pointer' }}
                        onClick={() => handleSelect(p.id)}
                      >
                        <Flex justify="between" align="center">
                          <Text size="2" weight="medium">{p.title}</Text>
                          <Badge size="1" variant="outline">{p.token_cost}t</Badge>
                        </Flex>
                        <Text size="1" color="gray">{p.id}</Text>
                      </Card>
                    ))}
                  </Flex>
                </Box>
              )}

              <Separator size="4" />

              {/* All pages */}
              <Flex justify="between" align="center">
                <Text size="2" weight="bold" color="gray">All Pages</Text>
                <Dialog.Root open={createOpen} onOpenChange={setCreateOpen}>
                  <Dialog.Trigger asChild>
                    <Button size="1" variant="soft">+ New</Button>
                  </Dialog.Trigger>
                  <Dialog.Portal>
                    <Dialog.Overlay style={{
                      position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)'
                    }} />
                    <Dialog.Content style={{
                      position: 'fixed', top: '50%', left: '50%',
                      transform: 'translate(-50%,-50%)',
                      background: 'var(--gray-2)', borderRadius: 8,
                      padding: 24, width: 420,
                    }}>
                      <Dialog.Title asChild>
                        <Heading size="4" mb="3">Create Page</Heading>
                      </Dialog.Title>
                      <Flex direction="column" gap="3">
                        <TextField.Root
                          placeholder="page-id (e.g. ai/transformers)"
                          value={newId}
                          onChange={(e) => setNewId(e.target.value)}
                        />
                        <TextField.Root
                          placeholder="Page Title"
                          value={newTitle}
                          onChange={(e) => setNewTitle(e.target.value)}
                        />
                        <TextArea
                          placeholder="Content with [[backlinks]]..."
                          value={newContent}
                          onChange={(e) => setNewContent(e.target.value)}
                          rows={6}
                        />
                        <Flex gap="2" justify="end">
                          <Dialog.Close asChild>
                            <Button variant="soft" color="gray">Cancel</Button>
                          </Dialog.Close>
                          <Button onClick={handleCreate}>Create</Button>
                        </Flex>
                      </Flex>
                    </Dialog.Content>
                  </Dialog.Portal>
                </Dialog.Root>
              </Flex>

              <ScrollArea style={{ maxHeight: '40vh' }}>
                <Flex direction="column" gap="1">
                  {pages.map((p) => (
                    <Card
                      key={p.id}
                      size="1"
                      style={{ cursor: 'pointer' }}
                      onClick={() => handleSelect(p.id)}
                    >
                      <Flex justify="between" align="center">
                        <Text size="2">{p.title}</Text>
                        <Flex gap="1">
                          {p.link_count > 0 && (
                            <Badge size="1" variant="outline" color="orange">
                              {p.link_count}
                            </Badge>
                          )}
                          <Badge size="1" variant="outline">{p.token_cost}t</Badge>
                        </Flex>
                      </Flex>
                      <Text size="1" color="gray">{p.id}</Text>
                    </Card>
                  ))}
                </Flex>
              </ScrollArea>

              <Separator size="4" />

              {/* REM Agent controls */}
              <Box>
                <Text size="2" weight="bold" color="gray">REM Agent</Text>
                <Flex direction="column" gap="2" mt="2">
                  <Button
                    variant="soft"
                    color="purple"
                    onClick={handleMaintain}
                    disabled={loading}
                  >
                    Run Maintenance Cycle
                  </Button>
                  {maintainResult && (
                    <Card size="1">
                      <DataList.Root size="1">
                        <DataList.Item>
                          <DataList.Label>Health</DataList.Label>
                          <DataList.Value>
                            <Badge color={maintainResult.health > 0.5 ? 'green' : 'red'}>
                              {(maintainResult.health * 100).toFixed(1)}%
                            </Badge>
                          </DataList.Value>
                        </DataList.Item>
                        <DataList.Item>
                          <DataList.Label>Pruned</DataList.Label>
                          <DataList.Value>{maintainResult.pruned_count}</DataList.Value>
                        </DataList.Item>
                        <DataList.Item>
                          <DataList.Label>Dreamed</DataList.Label>
                          <DataList.Value>{maintainResult.dreamed_count} edges</DataList.Value>
                        </DataList.Item>
                      </DataList.Root>
                      {maintainResult.dreamed_edges.length > 0 && (
                        <Box mt="2">
                          <Text size="1" color="purple">Suggested backlinks:</Text>
                          {maintainResult.dreamed_edges.map(([src, dst], i) => (
                            <Text key={i} size="1" as="div" color="gray">
                              {src} → {dst}
                            </Text>
                          ))}
                        </Box>
                      )}
                    </Card>
                  )}
                </Flex>
              </Box>
            </Flex>
          </Box>

          {/* Right panel: Page viewer */}
          <Box style={{ flex: 1 }}>
            {selected ? (
              <Card size="3">
                <Flex direction="column" gap="3">
                  <Box>
                    <Heading size="5">{selected.title}</Heading>
                    <Flex gap="2" mt="1">
                      <Badge variant="soft">{selected.id}</Badge>
                      <Badge variant="soft" color="orange">{selected.token_cost} tokens</Badge>
                    </Flex>
                  </Box>

                  {selected.links_to.length > 0 && (
                    <Box>
                      <Text size="2" color="gray" weight="bold">Backlinks</Text>
                      <Flex gap="1" mt="1" wrap="wrap">
                        {selected.links_to.map((link) => (
                          <Badge
                            key={link}
                            variant="soft"
                            color="cyan"
                            style={{ cursor: 'pointer' }}
                            onClick={() => handleSelect(link)}
                          >
                            {link}
                          </Badge>
                        ))}
                      </Flex>
                    </Box>
                  )}

                  <Separator size="4" />

                  <ScrollArea style={{ maxHeight: '60vh' }}>
                    <Text as="div" size="2" style={{ whiteSpace: 'pre-wrap', lineHeight: 1.7 }}>
                      {selected.content}
                    </Text>
                  </ScrollArea>
                </Flex>
              </Card>
            ) : (
              <Flex align="center" justify="center" style={{ height: '100%' }}>
                <Text color="gray" size="3">
                  Search or select a page to view
                </Text>
              </Flex>
            )}
          </Box>
        </Flex>
      </Flex>
    </Container>
  )
}
