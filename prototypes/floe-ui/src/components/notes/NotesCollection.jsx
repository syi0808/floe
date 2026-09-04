import { NotePreview } from './NotePreview.jsx';
import { useMemo, useState } from 'react';
import { Filter, Plus, Search } from 'lucide-react';
import { SQUIRCLE_RADIUS, SquircleButton, SquircleSurface } from '../../primitives.jsx';
import { initialNotes } from '../../data.js';

export function NotesCollection() {
  const [query, setQuery] = useState('');
  const [personalOnly, setPersonalOnly] = useState(false);
  const [notes, setNotes] = useState(initialNotes);

  const visibleNotes = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    return notes.filter((note) => {
      const matchesFilter = !personalOnly || note.category === 'Personal';
      const matchesQuery =
        !normalizedQuery ||
        [note.title, note.excerpt, note.category].some((value) =>
          value.toLowerCase().includes(normalizedQuery),
        );
      return matchesFilter && matchesQuery;
    });
  }, [notes, personalOnly, query]);

  function addNote() {
    if (notes.some((note) => note.id === 'untitled')) return;
    setNotes((current) => [
      {
        id: 'untitled',
        category: 'Draft',
        title: 'Untitled note',
        excerpt: 'Start writing a thought, decision, or detail you want to remember.',
        timestamp: 'Just now',
        tone: 'violet',
      },
      ...current,
    ]);
  }

  return (
    <div className="notes-screen">
      <section className="local-toolbar notes-toolbar">
        <h1>
          All notes <span>· {notes.length}</span>
        </h1>
        <div className="notes-tools">
          <SquircleSurface
            radius={SQUIRCLE_RADIUS.field}
            className="search-border"
            contentClassName="search-field"
          >
            <Search size={19} />
            <label className="sr-only" htmlFor="notes-search">
              Search notes
            </label>
            <input
              id="notes-search"
              type="search"
              placeholder="Search notes"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </SquircleSurface>
          <SquircleButton
            className={
              personalOnly
                ? 'secondary-button filter-button active'
                : 'secondary-button filter-button'
            }
            aria-pressed={personalOnly}
            onClick={() => setPersonalOnly((active) => !active)}
          >
            <Filter size={18} /> Filter
          </SquircleButton>
          <SquircleButton className="primary-button new-note-button" onClick={addNote}>
            <Plus size={18} /> New note
          </SquircleButton>
        </div>
      </section>

      {visibleNotes.length ? (
        <div className="notes-grid">
          {visibleNotes.map((note) => (
            <NotePreview key={note.id} note={note} />
          ))}
        </div>
      ) : (
        <div className="notes-empty">
          <Search size={24} />
          <h2>No notes found</h2>
          <p>Try a different search or clear the current filter.</p>
          <button
            type="button"
            className="violet-action"
            onClick={() => {
              setQuery('');
              setPersonalOnly(false);
            }}
          >
            Clear filters
          </button>
        </div>
      )}
    </div>
  );
}
