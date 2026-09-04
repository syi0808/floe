# Flutter ↔ HTML visual parity

## 기준

2026-09-04 기준 `prototypes/floe-ui`의 실제 브라우저 렌더링을 비교 기준으로 사용한다. 과거 mockup이나 문서의 추정 치수로 다시 해석하지 않는다.

- HTML: Chromium CDP, 1440×1100 / 390×844, DPR 1.
- Flutter: 동일 viewport의 실제 위젯 트리를 RepaintBoundary로 캡처, DPR 1.
- Today, Floe suggestion, Notes, Task Detail의 8쌍을 비교한다.
- 2026-09-03 14:28, 동일한 일정·노트·작업 상세 데이터를 사용한다.
- 브라우저에서 실제 선택된 폰트는 `Pretendard`였다. Flutter에도 Regular / SemiBold / Bold를 번들한다.

비교 화면: [스크린샷 갤러리](renders/flutter-parity/index.html)

## 수정한 차이

| 요소 | 이전 Flutter | 변경 |
|---|---|---|
| Shell | 별도 최대 콘텐츠 폭, 좁은 고정 rail | HTML의 1500 frame, 120/36 inset, 7:3 rail, 24 gap |
| Mobile | 고정 capture가 캘린더를 가림 | capture는 context 아래 문서 흐름, navigation만 고정 |
| Empty day | 별도 Now/Next hero | 동일한 빈 타임라인 구조 유지 |
| Squircle | ContinuousRectangleBorder, 0폭 hairline | Figma smoothing 0.82, semantic radius, BorderSide.none |
| Typography | OS 기본 폰트와 Material 기본 스타일 | 번들 Pretendard, 명시적 크기·행간·굵기 |
| Timeline | duration 높이, 짧은 일정의 overflow, 단색 블록 | 실제 reference의 compact block, dashed grid, tone metadata |
| Suggestion | 중앙 dialog / mobile bottom sheet | 타임 블록 위 floating button, anchor 기준 popup, 투명 backdrop |
| Context | leading checkbox와 삭제 메뉴, 목록형 note | tone dot / copy / trailing checkbox, white note card |
| Notes | 별도 검색 행과 정렬 버튼 | 반응형 toolbar, 3/2/1열 카드, category / excerpt / date |
| Task detail | 작은 제목, 다른 metadata 위계 | 48px title, metadata 행, subtasks, 동일한 rail |
| Mascot | raster asset | HTML과 동일한 SVG |

## 검증 가능한 치수

| Today timeline | HTML | Flutter |
|---|---|---|
| 1440px viewport, left / top | 137 / 163 | 137 / 163 |
| 390px viewport, left / top | 12 / 182 | 12 / 182 |
| timeline card height | 756 | 756 |
| all-day / hour step | 50 / 64 | 50 / 64 |
| mobile event inset | 68 / 18 | 68 / 18 |

이 값은 `visual_capture_test.dart`에서 검증한다. 픽셀 단위 완전 일치나 임의의 일치율을 주장하지 않는다. Chromium과 Flutter의 글꼴 rasterization, native scrollbar, 그림자에는 차이가 남을 수 있다.

## 의도적으로 유지한 차이와 데이터 경계

- HTML의 Progress는 개발 상태 dashboard이다. Flutter에는 기존 제품 navigation 3개만 유지한다.
- HTML Tasks는 고정된 상세 예시다. Flutter는 실제 task collection에서 상세로 이동한다. 캡처에서는 `Prepare launch brief` 항목을 선택한다.
- 실제 core 모델에는 note excerpt / category, task description / subtasks, event tone이 아직 없다. `DayAppearance`는 선택적인 presentation metadata이며 DB schema를 바꾸거나 임의의 예시를 실제 사용자 데이터로 넣지 않는다.
- Preview의 subtask checkbox는 메모리 상태만 바꾼다. 실제 task 완료·삭제·capture는 기존 gateway를 유지한다.
- Add break는 확인 후 gateway로 실제 이벤트를 추가한다. 중복/겹침을 검사하고 20분 창이 없으면 launcher를 숨긴다. AI 모델이 계산한 제안이 아니라 로컬 일정 간격 기반 UI이다.
- Week/Month, 노트 작성 및 상세 편집은 기존 미연결 상태를 유지한다. 이 작업은 backend 기능 완성이 아닌 UI parity 작업이다.

## 재현

프로토타입 개발 서버를 시작한 뒤 별도 터미널에서 Chrome CDP를 실행한다.

```sh
cd prototypes/floe-ui
npm run dev -- --host 127.0.0.1

"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless=new --disable-gpu --remote-debugging-port=9223 \
  --user-data-dir=/tmp/floe-parity-chrome about:blank
```

프로젝트 루트에서 Node 22+로 원본 HTML을 캡처한다. 스크립트는 viewport만 설정하며 reference CSS나 navigation을 숨기지 않는다.

```sh
node apps/client/tool/capture_prototype.mjs /tmp/floe-parity
cd apps/client
flutter test test/visual_capture_test.dart --dart-define=VISUAL_OUTPUT=/tmp/floe-parity
flutter run -d macos -t lib/main_preview.dart
```

`CHROME_CDP` / `PROTOTYPE_URL` 환경변수로 endpoint를 바꿀 수 있다. 프로토타입에는 폰트 번들이 없으므로 동일한 Pretendard 설치 환경에서 캡처해야 한다.

## Dependencies

- [figma_squircle](https://pub.dev/packages/figma_squircle): Figma corner smoothing.
- [Lucide Flutter](https://pub.dev/packages/lucide_icons_flutter): reference와 같은 icon family.
- [flutter_svg](https://pub.dev/packages/flutter_svg): shared mascot vector.
- [Pretendard](https://github.com/orioncactus/pretendard): OFL license는 `apps/client/assets/fonts/OFL.txt`에 포함.
