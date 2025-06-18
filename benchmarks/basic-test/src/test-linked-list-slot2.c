struct Node {
	int data;
	struct Node* next;
};

int getlast(struct Node* n) {
	struct Node* nxt = n->next;
	while(nxt != 0) {
		n = nxt;
		nxt = n->next;
	}
	return n->data;
}
