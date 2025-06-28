FROM python:3.10-slim
WORKDIR /app
COPY .. /app
RUN pip install --no-cache-dir -r requirements.txt
CMD ["python", "-c", "from inbox_agent.inbox_agent import InboxAgent; print('InboxAgent ready')"]
