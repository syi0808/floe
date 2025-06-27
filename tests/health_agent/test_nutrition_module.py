import datetime
from health_agent.nutrition_module import NutritionModule, MealRecord


def test_log_and_track():
    mod = NutritionModule()
    now = datetime.datetime.utcnow()
    meal = MealRecord(user_id="u", timestamp_utc=now, description="salad", calories=200, protein_g=5, carbs_g=10, fat_g=3)
    mod.log_meal("u", meal)
    meals = mod.get_meals_for_date("u", now.date())
    macros = mod.track_daily_nutrients(meals)
    assert macros.calories == 200
    assert macros.protein_g == 5
