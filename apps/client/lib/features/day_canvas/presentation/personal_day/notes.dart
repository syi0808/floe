part of '../personal_day_screen.dart';

class _NotesScreen extends StatefulWidget {
  const _NotesScreen({
    required this.notes,
    required this.narrow,
    required this.onCreate,
    required this.pending,
  });
  final List<NoteItem> notes;
  final bool narrow;
  final Future<bool> Function(String) onCreate;
  final bool pending;
  @override
  State<_NotesScreen> createState() => _NotesScreenState();
}

class _NotesScreenState extends State<_NotesScreen> {
  final search = TextEditingController();
  bool personalOnly = false;

  Future<void> _create() async {
    final saved = await showFloeDialog<bool>(
      context,
      (context) => _NewNoteDialog(save: widget.onCreate),
      barrierDismissible: false,
    );
    if (saved == true && mounted) {
      setState(() {
        search.clear();
        personalOnly = false;
      });
    }
  }

  @override
  void dispose() {
    search.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final query = search.text.trim().toLowerCase();
    final appearances = DayAppearance.of(context)?.notes ?? {};
    final notes = widget.notes.where((note) {
      final appearance = appearances[note.id];
      return (!personalOnly ||
              (appearance?.category ?? 'Personal') == 'Personal') &&
          [
            note.title,
            appearance?.excerpt ?? '',
            appearance?.category ?? '',
          ].any((text) => text.toLowerCase().contains(query));
    }).toList();
    final heading = Text(
      AppLocalizations.of(context).notesCount(widget.notes.length),
      style: FloeType.titleLarge.copyWith(height: 1.2),
    );
    final searchField = FloeSquircle(
      size: FloeSquircleSize.md,
      padding: EdgeInsets.symmetric(horizontal: 14),
      child: SizedBox(
        height: 50,
        child: Row(
          children: [
            Icon(LucideIcons.search, size: 19, color: FloePalette.neutral600),
            SizedBox(width: 10),
            Expanded(
              child: FloeSearchInput(
                controller: search,
                placeholder: AppLocalizations.of(context).searchNotes,
                onChanged: (_) => setState(() {}),
              ),
            ),
          ],
        ),
      ),
    );
    final filter = FloeButton.outlined(
      style: ButtonStyle(
        backgroundColor: WidgetStatePropertyAll(
          personalOnly ? FloePalette.primary100 : FloePalette.neutral0,
        ),
        side: const WidgetStatePropertyAll(
          BorderSide(color: FloePalette.neutral200),
        ),
      ),
      onPressed: () => setState(() => personalOnly = !personalOnly),
      icon: Icon(LucideIcons.filter, size: 18),
      child: Text(AppLocalizations.of(context).filter),
    );
    final create = FloeButton.filled(
      onPressed: widget.pending ? null : _create,
      loading: widget.pending,
      icon: Icon(LucideIcons.plus, size: 18),
      child: Text(AppLocalizations.of(context).newNote),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: EdgeInsets.only(top: 12, bottom: widget.narrow ? 16 : 12),
          child: widget.narrow || MediaQuery.sizeOf(context).width < 1000
              ? Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    heading,
                    SizedBox(height: FloeSpace.md),
                    searchField,
                    SizedBox(height: 10),
                    Row(
                      children: [
                        Expanded(child: filter),
                        SizedBox(width: 10),
                        Expanded(child: create),
                      ],
                    ),
                  ],
                )
              : Row(
                  children: [
                    Expanded(child: heading),
                    SizedBox(width: 240, child: searchField),
                    SizedBox(width: 10),
                    filter,
                    SizedBox(width: 10),
                    create,
                  ],
                ),
        ),
        SizedBox(height: widget.narrow ? 4 : 8),
        if (notes.isEmpty)
          Padding(
            padding: EdgeInsets.symmetric(vertical: 120),
            child: Center(
              child: Column(
                children: [
                  Icon(LucideIcons.search, size: 24),
                  SizedBox(height: FloeSpace.md),
                  Text(
                    AppLocalizations.of(context).noNotesFound,
                    style: FloeType.headline,
                  ),
                  SizedBox(height: FloeSpace.sm),
                  Text(
                    AppLocalizations.of(context).tryADifferentSearchOrClearThe,
                    textAlign: TextAlign.center,
                  ),
                  FloeButton.text(
                    onPressed: () => setState(() {
                      search.clear();
                      personalOnly = false;
                    }),
                    child: Text(AppLocalizations.of(context).clearFilters),
                  ),
                ],
              ),
            ),
          )
        else
          LayoutBuilder(
            builder: (context, constraints) {
              final columns = widget.narrow
                  ? 1
                  : MediaQuery.sizeOf(context).width > 1080
                  ? 3
                  : 2;
              final gap = widget.narrow ? 14.0 : 22.0;
              final width =
                  (constraints.maxWidth - gap * (columns - 1)) / columns;
              return Wrap(
                spacing: gap,
                runSpacing: gap,
                children: [
                  for (final note in notes)
                    SizedBox(
                      width: width,
                      child: _NotePreviewCard(
                        note: note,
                        onOpen: () => _openNote(context, note, widget.narrow),
                      ),
                    ),
                ],
              );
            },
          ),
      ],
    );
  }
}

class _NewNoteDialog extends StatefulWidget {
  const _NewNoteDialog({required this.save});
  final Future<bool> Function(String) save;

  @override
  State<_NewNoteDialog> createState() => _NewNoteDialogState();
}

class _NewNoteDialogState extends State<_NewNoteDialog> {
  final content = TextEditingController();
  bool pending = false;
  bool failed = false;

  @override
  void dispose() {
    content.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    if (pending || content.text.trim().isEmpty) return;
    setState(() {
      pending = true;
      failed = false;
    });
    final saved = await FloeLoading.run(() => widget.save(content.text.trim()));
    if (!mounted) return;
    if (saved) {
      Navigator.pop(context, true);
    } else {
      setState(() {
        pending = false;
        failed = true;
      });
    }
  }

  @override
  Widget build(BuildContext context) => PopScope(
    canPop: !pending,
    child: FloeDialog(
      title: Text(AppLocalizations.of(context).newNote),
      content: SizedBox(
        width: 420,
        child: FloeInput(
          key: Key('new-note-content'),
          label: AppLocalizations.of(context).writeAThoughtDecisionOrDetailTo,
          controller: content,
          autofocus: true,
          enabled: !pending,
          minLines: 3,
          maxLines: 8,
          onChanged: (_) => setState(() {}),
          errorText: failed
              ? AppLocalizations.of(context).couldNotSavePleaseTryAgain
              : null,
        ),
      ),
      actions: [
        FloeButton.text(
          onPressed: pending ? null : () => Navigator.pop(context),
          child: Text(AppLocalizations.of(context).cancel),
        ),
        FloeButton.filled(
          onPressed: pending || content.text.trim().isEmpty ? null : _save,
          loading: pending,
          child: Text(AppLocalizations.of(context).saveNote),
        ),
      ],
    ),
  );
}

class _NotePreviewCard extends StatelessWidget {
  const _NotePreviewCard({required this.note, required this.onOpen});
  final NoteItem note;
  final VoidCallback onOpen;
  @override
  Widget build(BuildContext context) {
    final appearance = DayAppearance.of(context)?.notes[note.id];
    final tone = appearance?.tone ?? ItemTone.violet;
    final mobile = MediaQuery.sizeOf(context).width <= 780;
    return FloeSquircle(
      fill: Color.lerp(Colors.white, tone.fill, .4)!,
      borderColor: tone.border,
      child: FloePressable(
        size: FloeSquircleSize.lg,
        onPressed: onOpen,
        child: Padding(
          padding: EdgeInsets.all(mobile ? 25 : 29),
          child: IntrinsicHeight(
            child: ConstrainedBox(
              constraints: BoxConstraints(minHeight: mobile ? 160 : 187),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      _ToneDot(color: tone.accent),
                      SizedBox(width: 10),
                      Text(
                        appearance?.category ??
                            AppLocalizations.of(context).personal,
                        style: FloeType.label.copyWith(
                          height: 1.2,
                          color: FloePalette.neutral600,
                        ),
                      ),
                    ],
                  ),
                  SizedBox(height: 20),
                  Text(
                    note.title,
                    style: FloeType.headline.copyWith(
                      fontWeight: FontWeight.w600,
                      height: 1.2,
                    ),
                  ),
                  SizedBox(height: 14),
                  Text(
                    appearance?.excerpt ?? '',
                    style: FloeType.body.copyWith(height: 1.65),
                  ),
                  Spacer(),
                  SizedBox(height: FloeSpace.lg),
                  Text(
                    appearance?.timestamp ?? _date(context, note.createdAt),
                    style: FloeType.bodySmall.copyWith(
                      fontSize: 12,
                      height: 1.2,
                      color: FloePalette.neutral500,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _NoteDetail extends StatelessWidget {
  const _NoteDetail({required this.note});
  final NoteItem note;
  @override
  Widget build(BuildContext context) => FloeSquircle(
    size: FloeSquircleSize.xl,
    padding: EdgeInsets.all(FloeSpace.xl),
    child: ConstrainedBox(
      constraints: BoxConstraints(maxWidth: 640),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Text(
                AppLocalizations.of(context).personalNote,
                style: FloeType.label,
              ),
              Spacer(),
              FloeButton.icon(
                tooltip: AppLocalizations.of(context).close,
                onPressed: () => Navigator.pop(context),
                icon: Icon(Icons.close),
              ),
            ],
          ),
          SizedBox(height: FloeSpace.base),
          Text(note.title, style: FloeType.display),
          SizedBox(height: FloeSpace.sm),
          Text(_date(context, note.createdAt), style: FloeType.numeric),
          SizedBox(height: FloeSpace.xl),
          Text(
            AppLocalizations.of(context).thisIsYourOriginalNoteEditingIs,
            style: FloeType.bodyLarge,
          ),
          SizedBox(height: FloeSpace.xl),
          FloeButton.outlined(
            onPressed: () => _showComingSoon(context),
            icon: FloeMascot(size: 24),
            child: Text(AppLocalizations.of(context).reviewWithFloe),
          ),
        ],
      ),
    ),
  );
}

class _ToneDot extends StatelessWidget {
  const _ToneDot({required this.color});
  final Color color;
  @override
  Widget build(BuildContext context) => DecoratedBox(
    decoration: BoxDecoration(color: color, shape: BoxShape.circle),
    child: SizedBox.square(dimension: 10),
  );
}
