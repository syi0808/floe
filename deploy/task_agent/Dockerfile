FROM python:3.10-slim
WORKDIR /app
COPY .. /app
RUN pip install --no-cache-dir -r requirements.txt
CMD ["python", "-c", "from task_agent.task_agent import TaskAgent; print('TaskAgent ready')"]
