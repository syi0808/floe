FROM python:3.10-slim
WORKDIR /app
COPY .. /app
RUN pip install --no-cache-dir -r requirements.txt
CMD ["python", "-c", "from schedule_agent.schedule_agent import ScheduleAgent; print('ScheduleAgent ready')"]
