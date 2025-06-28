FROM python:3.10-slim
WORKDIR /app
COPY .. /app
RUN pip install --no-cache-dir -r requirements.txt
CMD ["python", "-c", "from health_agent.health_agent import HealthAgent; print('HealthAgent ready')"]
