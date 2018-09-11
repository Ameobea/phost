from django.core import serializers
from django.http import JsonResponse


def serialize_model(model, json_response=True):
    data = serializers.serialize("python", [model])[0]
    serialized = {"id": data["pk"], **data["fields"]}

    if json_response:
        return JsonResponse(serialized, safe=False)
    else:
        return serialized


def serialize_models(models, json_response=True):
    data = serializers.serialize("python", models)
    serialized = list(map(lambda datum: {"id": datum["pk"], **datum["fields"]}, data))
    print(serialized)

    if json_response:
        return JsonResponse(serialized, safe=False)
    else:
        return serialized
