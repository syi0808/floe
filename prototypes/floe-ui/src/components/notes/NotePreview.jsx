import { SQUIRCLE_RADIUS, SquircleSurface } from '../../primitives.jsx';

export function NotePreview({ note }) {
  return (
    <SquircleSurface
      radius={SQUIRCLE_RADIUS.card}
      className={`note-preview-border tone-${note.tone}`}
      contentClassName="note-preview"
    >
      <div className="note-category">
        <span className={`tone-dot ${note.tone}`} /> {note.category}
      </div>
      <h2>{note.title}</h2>
      <p>{note.excerpt}</p>
      <time>{note.timestamp}</time>
    </SquircleSurface>
  );
}
