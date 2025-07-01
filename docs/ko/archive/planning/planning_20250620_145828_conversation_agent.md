# 대화 에이전트 계획 문서

** 날짜 : ** 20250620_145828

## 목적
기본 입력 처리, 대화 관리 및 데이터 모델을 포함한 대화 에이전트의 초기 설정.

## 개발 될 모듈

### 1.`Confermite_agent/common_types.py`
- ** 설명 : **이 모듈은 대화 상태, 사용자 입력 및 에이전트 응답을 관리하기위한 기본 Pydantic 모델을 정의합니다.
- ** 모델 : **
-`ConversationState ': 현재 대화 상태를 유지하려면 (예 : 역사, 맥락).
-`userInput ': 사용자로부터 수신 한 입력을 구조화합니다.
-`agentresponse ': 에이전트가 생성 한 응답을 구성합니다.

### 2.`Confermite_agent/input_handler.py`
- ** 설명 : **이 모듈은 사용자의 원시 입력을 처리 할 책임이 있습니다.처음에는 기본 텍스트 입력을 처리합니다.
- ** 기능 : **
- 텍스트 입력을받습니다.
- 기본 정규화 (예 : 스 트리밍 공백, 하단).
- 입력을 'userinput'모델로 패키지하십시오.

### 3.`conversation_agent/dialogue_manager.py`
- ** 설명 : **이 모듈은 대화 흐름과 상태를 관리합니다.
- ** 기능 : **
-`ConversationState`를 초기화하고 업데이트합니다.
- 입력 및 상태를 기반으로 응답을 생성하기위한 기본 논리 (처음에는 아마도 반향 또는 사전 정의 된 응답).
- 대화 역사를 저장하십시오.

## 단위 테스트

### 1.`tests/conversation_agent/test_input_handler.py`
- ** 목표 : ** 'Inputhandler'의 기능을 확인하십시오.
- ** 테스트 사례 : **
- 테스트 입력 정규화 (공백, 케이스 감도).
-`userinput '객체의 테스트 생성.

### 2.`tests/conversation_agent/test_dialogue_manager.py`
- ** 목표 : **`DialogUemanager`의 기능을 확인하십시오.
- ** 테스트 사례 : **
-`ConversationState '의 초기화를 테스트합니다.
- 테스트 상태 업데이트.
- 기본 응답 생성을 테스트하십시오.
- 테스트 기록 추적.

## 전달 가능
-`docs/planning_20250620_145828_conversation_agent.md` (이 문서)
-`convertion_agent/common_types.py '
-`Confermite_agent/input_handler.py`
-`convertion_agent/dialogue_manager.py '
-`tests/conversation_agent/test_input_handler.py`
-`tests/conversation_agent/test_dialogue_manager.py`
