part of '../personal_day_screen.dart';

class _NotesScreen extends StatefulWidget {
  const _NotesScreen({
    required this.notes,
    required this.date,
    required this.narrow,
    required this.onCreate,
    required this.pending,
  });
  final List<NoteItem> notes;
  final DateTime date;
  final bool narrow;
  final Future<bool> Function(String) onCreate;
  final bool pending;
  @override
  State<_NotesScreen> createState() => _NotesScreenState();
}

class _NotesScreenState extends State<_NotesScreen> {
  final search = TextEditingController();
  bool _restored = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_restored) return;
    _restored = true;
    final saved = PageStorage.maybeOf(context)
        ?.readState(context, identifier: 'notes-search');
    if (saved is String) search.text = saved;
  }

  void _rememberSearch() =>
      PageStorage.maybeOf(context)
          ?.writeState(context, search.text, identifier: 'notes-search');

  Future<void> _create() async {
    final saved = await _showNewNoteComposer(
      context: context,
      narrow: widget.narrow,
      save: widget.onCreate,
    );
    if (saved == true && mounted) {
      setState(() {
        search.clear();
        _rememberSearch();
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
    final notes =
        widget.notes
            .where((note) => note.title.toLowerCase().contains(query))
            .toList()
          ..sort((left, right) {
            final time = right.createdAt.compareTo(left.createdAt);
            return time != 0 ? time : left.id.compareTo(right.id);
          });
    final filtered = query.isNotEmpty;
    final copy = AppLocalizations.of(context);
    final dateLabel = DateFormat.yMMMd(copy.localeName).format(widget.date);
    final matchingLabel = filtered
        ? copy.notesMatchingCount(notes.length)
        : null;
    final visibleCount = filtered
        ? matchingLabel!
        : NumberFormat.decimalPattern(copy.localeName)
              .format(widget.notes.length);
    final countSemantics =
        matchingLabel ?? copy.notesCount(widget.notes.length);
    final heading = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          copy.notes,
          key: const Key('notes-page-title'),
          style: FloeType.pageTitle,
        ),
        const SizedBox(height: FloeSpace.xs),
        Wrap(
          spacing: FloeSpace.sm,
          runSpacing: FloeSpace.xxs,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            Semantics(
              label: countSemantics,
              child: ExcludeSemantics(
                child: Text(
                  visibleCount,
                  key: const Key('notes-count'),
                  style: FloeType.bodySmall,
                ),
              ),
            ),
            const ExcludeSemantics(child: Text('·')),
            Text(
              dateLabel,
              key: const Key('notes-selected-date'),
              style: FloeType.bodySmall,
            ),
          ],
        ),
      ],
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
                onChanged: (_) {
                  setState(() {});
                  _rememberSearch();
                },
              ),
            ),
          ],
        ),
      ),
    );
    final create = FloeButton.filled(
      key: const Key('notes-new-note'),
      onPressed: widget.pending ? null : _create,
      loading: widget.pending,
      icon: Icon(LucideIcons.plus, size: 18),
      child: Text(AppLocalizations.of(context).newNote),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: EdgeInsets.only(bottom: widget.narrow ? 16 : 12),
          child: widget.narrow || MediaQuery.sizeOf(context).width < 1000
              ? Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (MediaQuery.textScalerOf(context).scale(16) <= 20)
                      Row(
                        children: [
                          Expanded(child: heading),
                          SizedBox(width: 12),
                          create,
                        ],
                      )
                    else ...[
                      heading,
                      SizedBox(height: FloeSpace.md),
                      Align(alignment: Alignment.centerRight, child: create),
                    ],
                    SizedBox(height: FloeSpace.md),
                    searchField,
                  ],
                )
              : Row(
                  children: [
                    Expanded(child: heading),
                    SizedBox(width: 240, child: searchField),
                    SizedBox(width: 10),
                    create,
                  ],
                ),
        ),
        SizedBox(height: widget.narrow ? FloeSpace.sm : FloeSpace.md),
        if (notes.isEmpty)
          Padding(
            padding: EdgeInsets.symmetric(vertical: widget.narrow ? 48 : 80),
            child: Center(
              child: Column(
                children: [
                  FloeSquircle(
                    size: FloeSquircleSize.md,
                    fill: FloePalette.neutral100,
                    borderWidth: 0,
                    padding: const EdgeInsets.all(FloeSpace.base),
                    child: Icon(
                      filtered ? LucideIcons.search : LucideIcons.notebookPen,
                      size: 24,
                      color: FloeColor.textSecondary,
                    ),
                  ),
                  SizedBox(height: FloeSpace.md),
                  Text(
                    filtered ? copy.noNotesFound : copy.notesEmptyTitle,
                    style: FloeType.headline,
                  ),
                  SizedBox(height: FloeSpace.sm),
                  Text(
                    filtered ? copy.tryADifferentSearch : copy.notesEmptyBody,
                    textAlign: TextAlign.center,
                  ),
                  if (filtered)
                    FloeButton.text(
                      onPressed: () => setState(() {
                        search.clear();
                        _rememberSearch();
                      }),
                      child: Text(copy.clearSearch),
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
              final gap = widget.narrow ? FloeSpace.base : FloeSpace.lg;
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

Future<bool?> _showNewNoteComposer({
  required BuildContext context,
  required bool narrow,
  required Future<bool> Function(String) save,
}) {
  Widget page(BuildContext context) =>
      _NewNoteComposer(save: save, fullPage: true);

  if (!narrow) {
    return showFloeDialog<bool>(
      context,
      (context) => _NewNoteComposer(save: save),
      barrierDismissible: false,
    );
  }

  final route = FloeMotion.reduceMotion(context)
      ? PageRouteBuilder<bool>(
          pageBuilder: (context, _, _) => page(context),
          transitionDuration: Duration.zero,
          reverseTransitionDuration: Duration.zero,
          fullscreenDialog: true,
        )
      : MaterialPageRoute<bool>(builder: page, fullscreenDialog: true);
  return Navigator.of(context).push(route);
}

class _NewNoteComposer extends StatefulWidget {
  const _NewNoteComposer({required this.save, this.fullPage = false});
  final Future<bool> Function(String) save;
  final bool fullPage;

  @override
  State<_NewNoteComposer> createState() => _NewNoteComposerState();
}

class _NewNoteComposerState extends State<_NewNoteComposer> {
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
    final route = ModalRoute.of(context);
    setState(() {
      pending = true;
      failed = false;
    });
    final saved = await FloeLoading.run(() => widget.save(content.text.trim()));
    if (!mounted) return;
    if (saved) {
      _finishSave(route);
    } else {
      setState(() {
        pending = false;
        failed = true;
      });
    }
  }

  void _cancel() {
    if (pending) return;
    final route = ModalRoute.of(context);
    if (route == null || !route.isCurrent) return;
    Navigator.of(context).maybePop();
  }

  void _finishSave(ModalRoute<dynamic>? route) {
    if (route == null || !route.isActive) return;
    final navigator = Navigator.of(context);
    if (route.isCurrent) {
      navigator.pop(true);
    } else {
      // A save may finish while another route is above the composer. Remove
      // this route by identity so a late completion never pops that newer page.
      navigator.removeRoute(route, true);
    }
  }

  @override
  Widget build(BuildContext context) {
    final copy = AppLocalizations.of(context);
    final input = FloeInput(
      key: const Key('new-note-content'),
      label: copy.note,
      controller: content,
      autofocus: true,
      enabled: !pending,
      minLines: widget.fullPage ? null : 3,
      maxLines: widget.fullPage ? null : 8,
      expands: widget.fullPage,
      textAlignVertical: widget.fullPage ? TextAlignVertical.top : null,
      onChanged: (_) => setState(() {}),
      errorText: failed ? copy.couldNotSavePleaseTryAgain : null,
    );
    final actions = [
      FloeButton.text(
        key: const Key('new-note-cancel'),
        onPressed: pending ? null : _cancel,
        child: Text(copy.cancel),
      ),
      FloeButton.filled(
        key: const Key('new-note-save'),
        onPressed: pending || content.text.trim().isEmpty ? null : _save,
        loading: pending,
        child: Text(copy.saveNote),
      ),
    ];
    final Widget surface = widget.fullPage
        ? Scaffold(
            key: const Key('new-note-composer'),
            backgroundColor: FloeColor.surface,
            resizeToAvoidBottomInset: true,
            body: SafeArea(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Padding(
                    padding: const EdgeInsets.fromLTRB(24, 20, 24, 16),
                    child: Text(copy.newNote, style: FloeType.titleLarge),
                  ),
                  Expanded(
                    child: Padding(
                      padding: const EdgeInsets.fromLTRB(24, 0, 24, 16),
                      child: input,
                    ),
                  ),
                  Padding(
                    padding: const EdgeInsets.fromLTRB(24, 12, 24, 24),
                    child: Wrap(
                      alignment: WrapAlignment.end,
                      spacing: FloeSpace.sm,
                      runSpacing: FloeSpace.sm,
                      children: actions,
                    ),
                  ),
                ],
              ),
            ),
          )
        : FloeDialog(
            key: const Key('new-note-composer'),
            title: Text(copy.newNote),
            content: input,
            actions: actions,
            maxWidth: 640,
          );

    return PopScope(
      canPop: !pending,
      child: CallbackShortcuts(
        bindings: {const SingleActivator(LogicalKeyboardKey.escape): _cancel},
        child: surface,
      ),
    );
  }
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
    // The current note contract stores the complete original content in title.
    // Derive a bounded preview without changing that content or using a generated summary.
    final content = note.title.trim();
    final lineBreak = content.indexOf('\n');
    final heading = lineBreak < 0
        ? content
        : content.substring(0, lineBreak).trimRight();
    final excerpt = lineBreak < 0
        ? appearance?.excerpt ?? ''
        : content.substring(lineBreak + 1).trim();
    final timestamp = appearance?.timestamp ?? _time(context, note.createdAt);
    String brief(String value, int limit) {
      final text = value.replaceAll(RegExp(r'\s+'), ' ').trim();
      return text.characters.length > limit
          ? '${text.characters.take(limit)}…'
          : text;
    }

    return Semantics(
      key: ValueKey('note-preview-${note.id}'),
      button: true,
      enabled: true,
      label: [
        brief(heading, 96),
        brief(excerpt, 160),
        timestamp,
      ].where((text) => text.isNotEmpty).join('\n'),
      child: FloeSquircle(
        fill: Color.lerp(Colors.white, tone.fill, .4)!,
        borderColor: tone.border,
        child: FloePressable(
          size: FloeSquircleSize.lg,
          onPressed: onOpen,
          child: ExcludeSemantics(
            child: Padding(
              padding: const EdgeInsets.all(24),
              child: IntrinsicHeight(
                child: ConstrainedBox(
                  constraints: BoxConstraints(minHeight: mobile ? 124 : 151),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        heading,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: FloeType.headline.copyWith(
                          fontWeight: FontWeight.w600,
                          height: 1.2,
                        ),
                      ),
                      if (excerpt.isNotEmpty) ...[
                        SizedBox(height: 14),
                        Text(
                          excerpt,
                          maxLines: 3,
                          overflow: TextOverflow.ellipsis,
                          style: FloeType.body.copyWith(height: 1.65),
                        ),
                      ],
                      Spacer(),
                      SizedBox(height: FloeSpace.lg),
                      Text(
                        timestamp,
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
        ),
      ),
    );
  }
}

Future<void> _openNote(BuildContext context, NoteItem note, bool narrow) async {
  if (narrow) {
    Widget page(BuildContext context) => Scaffold(
      backgroundColor: FloeColor.surface,
      body: SafeArea(child: _NoteDetail(note: note, fullPage: true)),
    );
    final route = FloeMotion.reduceMotion(context)
        ? PageRouteBuilder<void>(
            pageBuilder: (context, _, _) => page(context),
            transitionDuration: Duration.zero,
            reverseTransitionDuration: Duration.zero,
            fullscreenDialog: true,
          )
        : MaterialPageRoute<void>(builder: page, fullscreenDialog: true);
    await Navigator.of(context).push(route);
  } else {
    await showFloeDialog<void>(
      context,
      (context) =>
          FloeDialogSurface(maxWidth: 640, child: _NoteDetail(note: note)),
    );
  }
}

class _NoteDetail extends StatelessWidget {
  const _NoteDetail({required this.note, this.fullPage = false});
  final NoteItem note;
  final bool fullPage;

  @override
  Widget build(BuildContext context) {
    final copy = AppLocalizations.of(context);
    final content = Column(
      key: const Key('note-reader'),
      mainAxisSize: fullPage ? MainAxisSize.max : MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 20, 16, 20),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Expanded(child: Text(copy.note, style: FloeType.titleLarge)),
                  FloeButton.icon(
                    tooltip: copy.close,
                    onPressed: () => Navigator.pop(context),
                    icon: const Icon(Icons.close),
                  ),
                ],
              ),
              const SizedBox(height: 4),
              Text(
                DateFormat.yMMMd(copy.localeName)
                    .format(note.createdAt.toLocal()),
                style: FloeType.caption.copyWith(
                  color: FloeColor.textSecondary,
                ),
              ),
            ],
          ),
        ),
        Flexible(
          child: SingleChildScrollView(
            key: const Key('note-reader-scroll'),
            padding: const EdgeInsets.fromLTRB(24, 0, 24, 28),
            child: SelectableText(
              note.title,
              key: const Key('note-original-content'),
              style: FloeType.bodyLarge.copyWith(height: 1.65),
            ),
          ),
        ),
      ],
    );
    final Widget surface = fullPage
        ? content
        : FloeSquircle(
            size: FloeSquircleSize.xl,
            child: ConstrainedBox(
              constraints: BoxConstraints(
                maxWidth: 640,
                maxHeight: MediaQuery.sizeOf(context).height * .85,
              ),
              child: content,
            ),
          );
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): () {
          unawaited(Navigator.of(context).maybePop());
        },
      },
      child: Focus(autofocus: true, child: surface),
    );
  }
}
