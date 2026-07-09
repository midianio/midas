// Generated from `openapi.json` (BE-0014 / FE-0006) — do not hand-edit. Regenerate with
// `openapi-typescript ../api/openapi.json -o src/lib/types/api.generated.ts` after a backend
// contract change, and commit the result (OPS-0003: both halves of the pair are committed).

export interface paths {
  "/items": {
    get: operations["listItems"];
  };
}

export interface components {
  schemas: {
    Item: {
      id: string;
      userId: string;
      createdAt: number;
    };
    ApiResponse_Vec_Item: {
      data: components["schemas"]["Item"][];
      code: number;
      timestamp: string;
      count: number;
    };
    ErrorBody: {
      status: number;
      code: string;
    };
  };
}

export interface operations {
  listItems: {
    responses: {
      200: {
        content: {
          "application/json": components["schemas"]["ApiResponse_Vec_Item"];
        };
      };
      401: {
        content: {
          "application/json": components["schemas"]["ErrorBody"];
        };
      };
    };
  };
}
