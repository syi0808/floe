# Product Boundaries

> Status: Accepted direction

## Floe가 아닌 것

### Agent Orchestration Framework가 아니다

멀티 Agent는 내부 구현 전략일 수 있지만 제품의 abstraction boundary는 아니다.

### Workflow Automation Product가 아니다

n8n/Activepieces처럼 사용자가 노드를 연결하는 workflow builder를 만들지 않는다.

외부 automation 제품의 **connector 구현 자산**에는 관심이 있지만 workflow abstraction은 Floe intelligence와 중복된다.

### ChatGPT Wrapper가 아니다

채팅은 중요한 surface일 수 있으나 제품의 메인 정보 구조가 아니다.

### Health Dashboard가 아니다

원시 건강 수치와 그래프를 계속 보여주는 것이 목표가 아니다.

Health data는 사용자의 하루를 더 잘 관리하기 위한 입력이다.

### Productivity Dashboard가 아니다

완료율, streak, 점수, badge로 삶을 gamification하지 않는다.

### Apple-only Product가 아니다

Apple 생태계를 깊게 지원하되 Windows/Android도 1급 플랫폼이다.

## 의도적으로 분리해야 하는 개념

- Account ≠ Person
- Skill ≠ Expert
- Expert ≠ Manager
- Integration ≠ Automation
- Connector ≠ MCP
- Memory ≠ Instruction
- Speaker recognition ≠ Authentication
- Intelligence ≠ Authority
- Local model ≠ Privacy policy
- Timeline UI projection ≠ underlying domain model
