#define _GNU_SOURCE

#include <pthread.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <ucontext.h>
#include "list.h"

// 进行切换通知的信号, 最好为不可靠信号, 避免信号堵住
// 这里和go保持一致性, 都是SIGURG
#define _CO_NOTIFY_SIGNO SIGURG
// main_exit栈的大小
#define _CO_EXIT_STACK_SIZE 4096
// 协程的栈大小
#define _CO_STACK_SIZE 128 * 1024
// 时间片大小
#define _CO_SLICE_NSEC 1000
// 最多执行多少个时间片
#define _CO_SCHEDULE_N_SLICE 10

// 协程函数的签名
typedef void (*co_func_t)(void*);

typedef enum _co_state {
    // 仅有两个状态, running和exit
    // exit状态代表此协程主动请求退出
    _CO_STATE_RUNNING,
    _CO_STATE_EXIT
} _co_state_t;

// 单个调度体
typedef struct _co_task_node {
    // 链表节点
    struct list_head node;
    // 单个协程的id
    int co_id;
    // 单个协程的上下文
    ucontext_t co_ctx;
    // 退出协程函数的上下文
    ucontext_t co_exit_ctx;
    // 协程当前状态
    _co_state_t co_state;
    // 为该协程分配的栈地址
    void* co_alloc_stack;
    void* exit_alloc_stack;
} _co_task_node_t;

// 调度相关的数据
typedef struct _co_schedule {
    // 被调度的协程, 这个协程是主协程
    _co_task_node_t task_main;

    // 当前正在运行的任务
    _co_task_node_t* cur_running_task;
    int n_task_running;

    // id号, 每次create的时候都递增它, 保证每个协程都有不同的id
    unsigned long id_inc;

    // monitor线程和程序运行线程之间的临界数据
    // 这里用锁保护
    pthread_mutex_t mutex;

} _co_schedule_t;

_co_schedule_t _co_sche;

void _co_main_exit() {
    exit(0);
}

void _co_signal_handler() {
    pthread_mutex_lock(&_co_sche.mutex);

    // 先一个一个找, 直到找到下一个
    _co_task_node_t* cur_task = _co_sche.cur_running_task;
    _co_task_node_t* iter_task = container_of(
        _co_sche.cur_running_task->node.next, _co_task_node_t, node);
    while (1) {
        if (iter_task->co_state == _CO_STATE_RUNNING) {
            break;
        } else {
            // 进行垃圾回收, 拿下一个对象
            free(iter_task->co_alloc_stack);
            free(iter_task->exit_alloc_stack);
            free(iter_task);
            list_del(&iter_task->node);

            iter_task = container_of(_co_sche.cur_running_task->node.next,
                                     _co_task_node_t, node);
            continue;
        }
    }

    // 找到了一个合适的切换对象, 现在进行切换

    _co_sche.cur_running_task = iter_task;

    pthread_mutex_unlock(&_co_sche.mutex);

    swapcontext(&cur_task->co_ctx, &iter_task->co_ctx);
}

// 在进入这个线程时, 它的所有信号都是屏蔽的
void* _main_thread_setup(void* data) {
    // 设置信号处理函数, 在信号处理函数中阻塞所有信号
    struct sigaction action;
    action.sa_flags = SA_SIGINFO | SA_RESTART;
    sigfillset(&action.sa_mask);
    action.sa_sigaction = _co_signal_handler;
    sigaction(_CO_NOTIFY_SIGNO, &action, NULL);

    // 现在可以处理信号了, 将接收调度信号, setcontext里面已经设置好
    // 了sigmask
    setcontext(&_co_sche.task_main.co_ctx);
    // unreachable
    return NULL;
}

void co_run(co_func_t func, void* arg) {
    // 下面进行一些初始化工作

    pthread_mutex_init(&_co_sche.mutex, NULL);
    _co_sche.n_task_running = 1;
    // id从2开始递增, 1以及被主协程使用
    _co_sche.id_inc = 2;

    void* main_stack = malloc(_CO_STACK_SIZE);
    void* main_exit_stack = malloc(_CO_EXIT_STACK_SIZE);
    _co_sche.task_main.co_alloc_stack = main_stack;
    _co_sche.task_main.exit_alloc_stack = main_exit_stack;

    _co_sche.task_main.co_id = 1;
    _co_sche.task_main.co_state = _CO_STATE_RUNNING;
    _co_sche.task_main.node.prev = _co_sche.task_main.node.next =
        &_co_sche.task_main.node;
    _co_sche.task_main.co_alloc_stack = main_stack;
    _co_sche.task_main.exit_alloc_stack = main_exit_stack;

    // 生成main协程的上下文
    getcontext(&_co_sche.task_main.co_exit_ctx);
    sigfillset(&_co_sche.task_main.co_exit_ctx.uc_sigmask);
    sigdelset(&_co_sche.task_main.co_exit_ctx.uc_sigmask, SIGURG);
    _co_sche.task_main.co_exit_ctx.uc_stack.ss_flags = 0;
    _co_sche.task_main.co_exit_ctx.uc_stack.ss_size = _CO_EXIT_STACK_SIZE;
    _co_sche.task_main.co_exit_ctx.uc_stack.ss_sp = main_exit_stack;
    makecontext(&_co_sche.task_main.co_exit_ctx, _co_main_exit, 0, NULL);

    getcontext(&_co_sche.task_main.co_ctx);
    sigfillset(&_co_sche.task_main.co_exit_ctx.uc_sigmask);
    sigdelset(&_co_sche.task_main.co_exit_ctx.uc_sigmask, SIGURG);
    _co_sche.task_main.co_ctx.uc_link = &_co_sche.task_main.co_exit_ctx;
    _co_sche.task_main.co_ctx.uc_stack.ss_flags = 0;
    _co_sche.task_main.co_ctx.uc_stack.ss_size = _CO_STACK_SIZE;
    _co_sche.task_main.co_ctx.uc_stack.ss_sp = main_stack;
    makecontext(&_co_sche.task_main.co_ctx, (void (*)())func, 1, arg);

    _co_sche.cur_running_task = &_co_sche.task_main;

    pthread_t tid;
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    // 得阻塞所有的信号, 然后在setup中设置新的信号处理函数
    sigset_t set;
    sigfillset(&set);
    pthread_attr_setsigmask_np(&attr, &set);
    pthread_create(&tid, &attr, _main_thread_setup, NULL);

    // 循环进行monitor
    int cur_running_co_id = 1;
    int slice_spent = 0;
    unsigned long sleep_time_nsec = _CO_SLICE_NSEC;
    struct timespec sleep_ts = {.tv_sec = 0, .tv_nsec = sleep_time_nsec};
    while (1) {
        nanosleep(&sleep_ts, NULL);
        pthread_mutex_lock(&_co_sche.mutex);
        // 确定此时只有一个线程在被调度
        if (_co_sche.n_task_running == 1 &&
            _co_sche.cur_running_task->co_id == 1) {
            pthread_mutex_unlock(&_co_sche.mutex);
            continue;
        }
        if (cur_running_co_id != _co_sche.cur_running_task->co_id) {
            cur_running_co_id = _co_sche.cur_running_task->co_id;
            slice_spent = 0;
            pthread_mutex_unlock(&_co_sche.mutex);
            continue;
        }
        slice_spent++;
        if (slice_spent > _CO_SCHEDULE_N_SLICE) {
            sigval_t v;
            int n = pthread_sigqueue(tid, _CO_NOTIFY_SIGNO, v);
        }
        pthread_mutex_unlock(&_co_sche.mutex);
    }
}

// 仅仅将自己标记为_CO_STATE_EXIT, 真正的内存回收工作在下次调度时扫描进行
// 标记完成后, 释放锁, 原地while(1)等待
void _co_exit() {
    // 先阻塞信号
    sigset_t newmask;
    sigset_t oldmask;
    sigfillset(&newmask);
    pthread_sigmask(SIG_SETMASK, &newmask, &oldmask);

    pthread_mutex_lock(&_co_sche.mutex);
    _co_sche.n_task_running--;
    _co_sche.cur_running_task->co_state = _CO_STATE_EXIT;
    pthread_mutex_unlock(&_co_sche.mutex);

    pthread_sigmask(SIG_SETMASK, &oldmask, NULL);

    while (1)
        ;
}

void co_create(co_func_t func, void* arg) {
    // 在锁之前设置信号阻塞, 否则会发生死锁
    sigset_t newmask;
    sigset_t oldmask;
    sigfillset(&newmask);
    pthread_sigmask(SIG_SETMASK, &newmask, &oldmask);

    pthread_mutex_lock(&_co_sche.mutex);
    // 此处增加一个协程
    _co_task_node_t* newtask = malloc(sizeof(_co_task_node_t));
    newtask->co_state = _CO_STATE_RUNNING;
    newtask->co_id = _co_sche.id_inc;
    _co_sche.id_inc++;
    _co_sche.n_task_running++;

    void* co_stack = malloc(_CO_STACK_SIZE);
    void* exit_stack = malloc(_CO_EXIT_STACK_SIZE);
    newtask->co_alloc_stack = co_stack;
    newtask->exit_alloc_stack = exit_stack;

    // 获得新的上下文
    getcontext(&newtask->co_exit_ctx);
    newtask->co_exit_ctx.uc_stack.ss_flags = 0;
    sigfillset(&newtask->co_exit_ctx.uc_sigmask);
    sigdelset(&newtask->co_exit_ctx.uc_sigmask, _CO_NOTIFY_SIGNO);
    newtask->co_exit_ctx.uc_link = NULL;
    newtask->co_exit_ctx.uc_stack.ss_flags = 0;
    newtask->co_exit_ctx.uc_stack.ss_size = _CO_EXIT_STACK_SIZE;
    newtask->co_exit_ctx.uc_stack.ss_sp = exit_stack;
    makecontext(&newtask->co_exit_ctx, _co_exit, 0);

    getcontext(&newtask->co_ctx);
    newtask->co_ctx.uc_stack.ss_flags = 0;
    sigfillset(&newtask->co_ctx.uc_sigmask);
    sigdelset(&newtask->co_ctx.uc_sigmask, _CO_NOTIFY_SIGNO);
    newtask->co_ctx.uc_link = &newtask->co_exit_ctx;
    newtask->co_ctx.uc_stack.ss_flags = 0;
    newtask->co_ctx.uc_stack.ss_size = _CO_STACK_SIZE;
    newtask->co_ctx.uc_stack.ss_sp = co_stack;
    makecontext(&newtask->co_ctx, (void (*)(void))func, 1, arg);

    // 将新的任务加入到链表
    list_add_tail(&newtask->node, &_co_sche.task_main.node);

    pthread_mutex_unlock(&_co_sche.mutex);

    // 恢复信号处理
    pthread_sigmask(SIG_SETMASK, &oldmask, NULL);
}

// -----------------------
// 上面是库函数

void my_new_func2(void* p) {
    int n = 1;
    while (n < 60) {
        if (n % 10 == 0) {
            printf("my_new_func2 = %d\n", ff(n));
        }
        n++;
    }
}

void my_new_func1(void* p) {
    int n = 1;
    while (n < 60) {
        if (n % 7 == 0) {
            printf("my_new_func1 = %d\n", ff(n));
        }
        n++;
    }
}

void my_main_func(void* p) {
    co_create(my_new_func1, NULL);
    co_create(my_new_func2, NULL);
    int n = 48;
    while (n > 0) {
        if (n % 2 == 0) {
            printf("my_main_func = %d\n", ff(n));
        }
        n--;
    }
}

int ff(int n) {
    if (n == 1) {
        return 1;
    }
    if (n == 2) {
        return 1;
    } else {
        return ff(n - 1) + ff(n - 2);
    }
}

int main() {
    co_run(my_main_func, NULL);
}