from functools import partial

from django.core import serializers
from django.db.models import QuerySet
from django.http import JsonResponse


def serialize(model, json=True):
    serialized = None
    if isinstance(model, list) or isinstance(model, QuerySet):
        serialized = list(map(partial(serialize, json=False), model))
    else:
        data = serializers.serialize("python", [model])[0]
        serialized = {"id": data["pk"], **data["fields"]}

    if json:
        return JsonResponse(serialized, safe=False)
    else:
        return serialized
