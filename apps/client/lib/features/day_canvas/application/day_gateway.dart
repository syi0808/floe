import '../domain/day_models.dart';

abstract interface class DayGateway {
  Future<DaySnapshot> loadDay(DayQuery query);
  Future<CaptureReceipt> submitCapture(String input, DayQuery query);
  Future<DaySnapshot> classifyCapture(
    CaptureReceipt capture,
    ClassificationDraft classification,
    DayQuery query,
  );
  Future<DaySnapshot> setTaskCompleted(
    TaskItem task,
    bool completed,
    DayQuery query,
  );
  Future<DaySnapshot> deleteItem(DayItem item, DayQuery query);
}
