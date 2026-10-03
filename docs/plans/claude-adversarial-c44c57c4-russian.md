# Claude adversarial review: first response

Source: local Claude Code 2.1.287, requested model claude-opus-5-5, effort high. Input c44c57c4879c38034d82a040fad22ba43a606e7c; pre-removal plan/T0 snapshot, NOT current S1. Original Russian response preserved, independently read from completed output. Exit 0 reported. Findings are reviewer claims requiring source verification, not automatically accepted defects. A second Korean response is separate and must not erase conflicting findings.

---

# Ревью архива c44c57c4 (baseline 3f4b407): план полного cutover и T0

Это только статический анализ архивного среза. Команды не запускались, файлы не менялись. Текущую cloud-реализацию S1 я не видел и о ней ничего не утверждаю. Пути ниже даны относительно корня архива.

## Actionable findings

### F1 — High · установленный дефект плана. Нет нативного примитива для записи Gateway credentials, а не-macOS заглушка выдаёт «Gateway не настроен»

- **Где:**
  - Контракт `GatewayCredentialStore::{read,commit,remove}` в `docs/plans/2026-10-02-architecture-refactor.md:117-134` должен быть «implemented through `crates/platform/native/src/keychain.rs`».
  - В `canonical-contracts.md:112` `NoGatewayConfigured` разрешён только после «successful exact local credential-slot read».
  - Сам `crates/platform/native/src/keychain.rs:17-47` умеет только читать (`read_generic_password`). Ветка `#[cfg(not(target_os="macos"))]` на `:49-56` безусловно возвращает `Ok(None)`.
  - В `rust-symbol-map.json:647072-647089` этот файл помечен KEEP с формулировкой «No hidden policy or new feature introduced». При этом `:9280` требует «add atomic idempotent install/readback».
  - Удаляемый Swift-писатель (`apps/client/macos/Runner/MainFlutterWindow.swift:99-113`, iOS `AppDelegate.swift:51-55`) ставит `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` и читает с `interactionNotAllowed` (`:90-92`). Ни одно из этих свойств в целевом контракте не зафиксировано. Go-сторона своё «device-local accessibility» сохраняет явно (`server-symbol-map.md:3193`).
- **Контрпример:**
  - Сборка под iOS: `read` → `Ok(None)` → `PrimaryAbsence::NoGatewayConfigured` → тихий local fallback. Ожидалось «unsupported», то есть `Err`.
  - macOS, S1.2: `commit` нечем реализовать. Целевая map запрещает менять `keychain.rs`, а ветка «or a narrow native driver» нигде не размечена.
  - Новый писатель без явного атрибута accessibility создаст item с системной accessibility по умолчанию, без `ThisDeviceOnly`.
- **Последствия:** блокирует S1.2 и противоречит закрытию R1/R5. Возможна регрессия device-binding у bearer.
- **Коррекция:**
  - В S1.1 добавить в map native-примитивы `write/replace/delete` с exact-slot, `ThisDeviceOnly` и interaction-not-allowed.
  - Не-macOS ветку сделать `Err(Unsupported)`, а не `Ok(None)`. Заодно пересмотреть `skip_authenticated_items(true)` (`:32`): сейчас недоступный item сворачивается в «отсутствует».

### F2 — Medium · установлен (утраченное D-поведение). У чтения и commit credentials нет deadline/отмены

- **Где:**
  - `t0-client-behavior-ledger-connections.md:485-493`: тест local_server_test#2 «stalled Keychain read» классифицирован как D (durable). Ожидание: timeout без delete/forget.
  - Целевой `GatewayCredentialStore` синхронный и без scope (`architecture-refactor.md:122-131`).
  - `GatewayCredentialCommit::commit/readback` тоже без `OperationScope` (`rust-cutover.md:145-146`).
  - Baseline-чтение в Rust (`server_connection.rs:186`) тоже синхронное и без таймаута. Значит, единственная защита жила во Flutter, а Flutter-путь удаляется.
- **Контрпример:** Keychain зависает → `observe_primary(scope)` блокируется в синхронном вызове, и `scope` с дедлайном не помогает. Pairing commit тоже висит. Ожидалось: `DeadlineExceeded` без удаления credential.
- **Коррекция:** async-обёртка с bounded deadline (отдельный поток или очередь), `scope` в обоих портах и явный `CredentialStoreError::Timeout`, который не превращается в absence.

### F3 — High · установленный пробел контракта. Нет публичного метода, создающего Access review descriptor для заблокированной проекции

- **Где:**
  - `rust-cutover.md:118-123`: `NeedsSourceReview(SourceProjectionReview{projection_operation_id,target_digest,blockers})`, без `ReviewRef`.
  - `vault-cutover.md:103,121`: «Access review descriptors may already exist independently».
  - В `canonical-contracts.md:245-248,352` есть только `prepare_observe_review(command_id, source_ref, expected_revision, requested_processing)`. Это один источник, без consumer/categories. Плюс resolution-вызовы.
  - `app-symbol-map.json:65606/65790` переносит baseline `capture_inline` в *private* helper Access и требует «delegate any subject identity mutation to explicit Connections command before capture».
  - Baseline-путь: `crates/app/src/vault_host/conversation_turn/interaction_publication.rs:366-376`.
- **Контрпример:** блокер на 2 источника с `GatewayAllowed{highly_sensitive}` для `conversation.root`. Ни одна каноническая операция не создаёт `ConnectionReview` с идемпотентностью по `projection_operation_id`. `apply_observe(review_ref)` нечего передать. Если разрешить скрытую мутацию subject внутри Run, это нарушит «no query allocates durable meaning» (`:218`).
- **Последствия:** S1.3/S1.5 нельзя реализовать по «exact» контракту. Заявленное закрытие R2/R4 преувеличено.
- **Коррекция:** добавить `AccessService::prepare_projection_review(actor, projection_operation_id, blockers, plan) -> Vec<ReviewRef>`. Идемпотентность по `(run_id, projection_operation_id, target_digest)`. Явно запретить мутацию SourceAuthority внутри этого вызова. Пока subject не обновлён, отдавать navigation-only.

### F4 — Medium · установленное противоречие владения. Транзакция Access «инвалидирует» Actions

- **Где:**
  - `vault-cutover.md:88` («invalidates only not-yet-dispatched Actions» внутри commit Access) и `:145` (атомарная единица Access включает «invalidation of pending/approved effects»).
  - Против этого: `canonical-contracts.md:391,398,408` (рёбер Access↔Actions нет) и `app-cutover.md:216,246-248` (единственный владелец записи и state machine — `ActionsRepository`/Actions).
  - Нарушается также правило `module-dependencies.json:325`: shared storage не даёт права владеть чужими таблицами.
- **Контрпример:** disconnect при Action в состоянии Approved. Либо Vault-адаптер внутри SQL Access переводит Actions-строку в Blocked без валидатора Actions, то есть в обход state machine. Либо Action остаётся Approved, и единственной защитой служит `current_source_fence` в `prepare_dispatch`. Что именно, спецификация не говорит.
- **Коррекция:** убрать инвалидацию Actions из `GrantCommit`. Явно сделать source-reservation fence необходимым условием `prepare_dispatch`, плюс отдельный Actions-owned reconcile (Approved→Blocked{SourceChanged}) после receipt Connections.

### F5 — Medium · установленный пробел. Гонка cancel_pairing против уже активированного pairing на Go не определена

- **Где:**
  - Go атомарно активирует issuer и client credential (`architecture-refactor.md:138`, `server-cutover.md:73`). После этого Rust делает Verifying→Committing.
  - `cancel` (`architecture-refactor.md:152-153`, `canonical-contracts.md:233`) не определяет поведение после remote approval.
  - Для integrations та же гонка описана явно (`canonical-contracts.md:278`).
- **Контрпример:** администратор одобрил → пользователь жмёт Cancel до локального commit. Rust → `cancelled`, staged bearer выброшен. На Go остаётся активный client и issuer. Повторный pairing того же owner key может упереться в «sole active issuer» или в отказ на resurrection.
- **Коррекция:** после `AwaitingApproval→approved` cancel превращается в явный `RevokeClient` с `remote_revocation_pending`. Либо cancel отклоняется после `Verifying` с требованием forget/revoke.

### F6 — Medium · установлено. Production-проба модели в dashboard несовместима с новым `InvokeStructured`

- **Где:**
  - Baseline проверяет *любой* настроенный target по ID через Agent-маршрут: `server/internal/application/test_target.go:20,29` и `inference/synthetic.go:15-31`.
  - Цель: `InvokeStructured(ctx, OperatorPrincipal, {purpose, capability_revision…})` (`canonical-contracts.md:172,201,203`). Ревизия считается по purpose-маршруту.
  - План проб: `server-symbol-map.md:64-68`, «Keep dashboard Test functionality».
  - Дополнительно: «startup probes … do not fabricate a Principal» (`:201`), но zero `OperatorPrincipal` невалиден.
- **Контрпример:** target «B» сохранён, но ещё не назначен ни одному purpose → для него нет `capability_revision` → протестировать его нельзя. Agent-путь (tools/messages) вообще перестаёт проверяться.
- **Коррекция:** отдельный операторский метод `ProbeTarget(OperatorPrincipal, TargetID)` с явной bounded-семантикой Agent-запроса и без capability fence. Отдельно определить путь startup self-check.

### F7 — Medium · дефект baseline установлен, эксплуатируемость в целевой системе не проверена. Microsoft refresh снимает quarantine

- **Где:**
  - `server/internal/microsoftauth/runtime.go:180-184`: при смене subject `Token()` сохраняет новый `ProviderIdentity` с `IdentityVerified=false`, но **без** `IdentityReviewRequired`. Следующий `ProviderIdentity()` (`:199-222`) сравнивает уже подменённое значение и помечает новый аккаунт verified.
  - Google ставит флаг (`googleauth/runtime.go:188-193`).
  - T0 это нашёл (`t0-go-oauth-semantic-audit.json:96`), но в список дефектов плана (`server-cutover.md:216-229`, `architecture-refactor.md:46-63`) не перенёс. Map предписывает «Preserve … identity quarantine» (`server-symbol-map.md:3749`). Тест удаляется (`:3895`).
- **Контрпример:** refresh возвращает другой subject → два вызова → новый аккаунт verified без review.
- **Коррекция:** внести в §6 и S2.1 как target fix: sticky `IdentityReviewRequired` в `Token()`. Отдельно проверить, что authority fence сравнивает identity с неизменяемой записью source, а не со статусом runtime.

### F8 — Medium · пробел контракта. Inference не отличает «никогда не сопрягали» от «сопряжено, но credential потерян»

- **Где:** `canonical-contracts.md:112` и `rust-cutover.md:202`: absence определяется только чтением слота. Pin в Vault (`vault-cutover.md:169`) и pairing-состояние Connections в это решение не входят.
- **Контрпример:** Keychain item удалён вне продукта, а Vault pin и Connections `connected` остались. Тогда `NoGatewayConfigured` → тихий local fallback при UI «connected». Ожидалось `Err(RepairRequired)`.
- **Коррекция:** non-secret binding-маркер (generation) вне слота, доступный Inference без ребра на Connections, например через Access `GatewayTrustReader`. Пустой слот при живом маркере — ошибка.

### F9 — Low

- **Сигнатура `GatewayPairingPort`:** три разные формы. `async fn` в `architecture-refactor.md:145-154`, `impl Future + Send` (не dyn-compatible) в `rust-cutover.md:140-143`, а в canonical её нет вообще. Это противоречит заявленной «exact» closure R4.
- **Сброс профиля и тот же слот:** слот `app.floe.local-server/connection-v1` сохраняется (`:136`), а payload становится несовместимым. Старый item → вечное fail-closed без owner-пути удаления, так как forget требует generation. Спасает только внешний `reset-local-data.sh`.
- **Дрейф статусов T0:** `t0-execution-status.json:52` (`blocking_removal_gates: []`) расходится с `:64` (bindings «exact item spans must be completed before their edits») и с `coordinator-disposition.json:13`. VA-F01 в `t0-vault-removal-audit.md:14` устарел: заголовок ledger уже исправлен.

## Оценка R1–R8

| | Статус | Основание |
|---|---|---|
| R1 | частично | Wire schema 2 и таблица статусов согласованы. Но absence зависит от хранилища credentials (F1, F8); не определено, как оператору пробовать конкретный target (F6). |
| R2 | частично | Blocked/`finish_blocked_run`/`claim_resume` согласованы с baseline: `RunState` с 6 вариантами становится 7. Нет пути создания review (F3). Восстановление решения в состоянии `Resolving` после краха — только через явный `refresh_interaction` (`app-cutover.md:61-62,155`). |
| R3 | частично | Протокол `GrantOperationSlot` commit/abort корректен. Противоречие владения Actions (F4). |
| R4 | частично | Таблицы есть. Пробелы: F3, F5, F9. |
| R5 | не доказано полностью | Выборочно подтверждено: `remote_authority.rs:2505-2547`, `agent_actions.rs:463-495`. Контрпример — KEEP для `keychain.rs` (F1). |
| R6 | закрыто | Порядок стадий согласован в `client-cutover.md:162-169`. |
| R7 | закрыто | DAG сверен с манифестами (knowledge/conversation/actions/experts/context/providers/vault/runtime) и `module-dependencies.json`. Циклов нет, пути Inference↔Connections нет. Промежуточные рёбра S1 исчерпывающе не проверял. |
| R8 | закрыто как дизайн | С оговоркой F4. Текст R8 о сохранении старых записей отменён явно (`plan-review.md:7`). Ограничение живучести (Low): краш между `prepare_dispatch` и handoff оставляет Action навсегда `Unknown` (lookup-only). |

## Что не проверено

- Карты Flutter client/presentation и feature-UI.
- Построчные Go integrations/authority/views.
- Bindings ABI/header.
- Health Swift (`HealthPrivacyTransform.swift` в baseline нет, это новый файл).
- Семантика 150 записей Context.
- Счёты регистраций T0: проверена только арифметика сумм (751/131/1298).
- Реальное поведение ACL Keychain и процессов.
- `PreparedModelTransport::generate` без `scope`: так же и в baseline, это не регрессия; отмена работает только через drop future.
- Любые runtime-свойства.

